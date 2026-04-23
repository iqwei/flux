use std::sync::Arc;
use std::time::Duration;

use flux_proto::{Snapshot, SNAPSHOT_SCHEMA_VERSION};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::{interval, timeout, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use crate::clock::Clock;
use crate::config::ServerConfig;
use crate::health::HealthTracker;
use crate::ingest::Event;
use crate::metric_store::MetricStore;

pub(crate) async fn aggregator_task(
    mut rx: mpsc::Receiver<Event>,
    snap_tx: broadcast::Sender<Arc<Snapshot>>,
    final_snapshot_tx: watch::Sender<Option<Arc<Snapshot>>>,
    cfg: ServerConfig,
    clock: Arc<dyn Clock>,
    mut ingress_done: oneshot::Receiver<()>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let started_at = clock.now();
    let window = Duration::from_millis(cfg.rolling_window_ms);
    let shutdown_budget = Duration::from_millis(cfg.shutdown_budget_ms);
    let mut store = MetricStore::new(window, started_at);
    let mut health = HealthTracker::new(window, started_at);

    let mut ticker = interval(Duration::from_millis(cfg.broadcast_interval_ms));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
    ticker.tick().await;

    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => {
                if !drain_until_ingress_stops(
                    &mut rx,
                    &mut store,
                    &mut health,
                    clock.as_ref(),
                    &mut ingress_done,
                    shutdown_budget,
                )
                .await
                {
                    tracing::warn!(
                        budget_ms = cfg.shutdown_budget_ms,
                        "aggregator: shutdown budget exceeded before ingress stopped"
                    );
                }
                drain(&mut rx, &mut store, &mut health, clock.as_ref());
                let snap = Arc::new(build_snapshot(&mut store, &mut health, clock.as_ref()));
                let _ = final_snapshot_tx.send(Some(Arc::clone(&snap)));
                let _ = snap_tx.send(snap);
                tracing::debug!("aggregator: final snapshot emitted, exiting");
                return Ok(());
            }
            _ = ticker.tick() => {
                let snap = build_snapshot(&mut store, &mut health, clock.as_ref());
                let _ = snap_tx.send(Arc::new(snap));
            }
            maybe = rx.recv() => match maybe {
                Some(event) => apply_event(event, &mut store, &mut health, clock.as_ref()),
                None => return Ok(()),
            }
        }
    }
}

async fn drain_until_ingress_stops(
    rx: &mut mpsc::Receiver<Event>,
    store: &mut MetricStore,
    health: &mut HealthTracker,
    clock: &dyn Clock,
    ingress_done: &mut oneshot::Receiver<()>,
    shutdown_budget: Duration,
) -> bool {
    let drain_future = async {
        loop {
            drain(rx, store, health, clock);
            tokio::select! {
                biased;
                res = &mut *ingress_done => {
                    if res.is_err() {
                        tracing::debug!("aggregator: ingress completion signal dropped");
                    }
                    break;
                }
                maybe = rx.recv() => match maybe {
                    Some(event) => apply_event(event, store, health, clock),
                    None => break,
                }
            }
        }
    };

    timeout(shutdown_budget, drain_future).await.is_ok()
}

fn drain(
    rx: &mut mpsc::Receiver<Event>,
    store: &mut MetricStore,
    health: &mut HealthTracker,
    clock: &dyn Clock,
) {
    while let Ok(event) = rx.try_recv() {
        apply_event(event, store, health, clock);
    }
}

fn apply_event(
    event: Event,
    store: &mut MetricStore,
    health: &mut HealthTracker,
    clock: &dyn Clock,
) {
    match event {
        Event::Packet(packet, at) => {
            health.on_packet(at);
            // last_update_ms is stamped with server wall clock, not packet.timestamp_ms,
            // so staleness detection is robust to producer clock skew.
            store.ingest(packet, at, clock.unix_ms());
        }
        Event::ParseError => health.on_parse_error(),
        Event::SubscriberJoin => health.on_subscriber_join(),
        Event::SubscriberLeave => health.on_subscriber_leave(),
    }
}

fn build_snapshot(
    store: &mut MetricStore,
    health: &mut HealthTracker,
    clock: &dyn Clock,
) -> Snapshot {
    let now = clock.now();
    let generated_at_ms = clock.unix_ms();
    let metrics = store.summaries(now);
    let pipeline = health.snapshot(now);
    Snapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        generated_at_ms,
        uptime_ms: pipeline.uptime_ms,
        metrics,
        health: pipeline,
    }
}
