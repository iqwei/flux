use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use flux_proto::Snapshot;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

use crate::ingest::Event;

#[derive(Debug)]
pub(crate) struct WsState {
    pub(crate) events: mpsc::Sender<Event>,
    pub(crate) snapshots: broadcast::Sender<Arc<Snapshot>>,
}

pub(crate) async fn ws_task(
    listener: TcpListener,
    state: Arc<WsState>,
    final_snapshot: watch::Receiver<Option<Arc<Snapshot>>>,
    final_snapshot_wait: Duration,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let shutdown = cancel.clone();
    let router = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(AppState {
            state,
            cancel,
            final_snapshot,
            final_snapshot_wait,
        });
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { shutdown.cancelled().await })
        .await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    state: Arc<WsState>,
    cancel: CancellationToken,
    final_snapshot: watch::Receiver<Option<Arc<Snapshot>>>,
    final_snapshot_wait: Duration,
}

async fn ws_handler(ws: WebSocketUpgrade, State(app): State<AppState>) -> Response {
    ws.on_upgrade(move |socket| {
        run_subscriber(
            socket,
            app.state,
            app.cancel,
            app.final_snapshot,
            app.final_snapshot_wait,
        )
    })
}

const CLOSE_CODE_NORMAL: u16 = 1000;
const CLOSE_CODE_POLICY: u16 = 1008;

async fn run_subscriber(
    mut socket: WebSocket,
    state: Arc<WsState>,
    cancel: CancellationToken,
    mut final_snapshot: watch::Receiver<Option<Arc<Snapshot>>>,
    final_snapshot_wait: Duration,
) {
    let mut rx = state.snapshots.subscribe();
    let events = state.events.clone();
    if let Err(err) = events.try_send(Event::SubscriberJoin) {
        tracing::warn!(%err, "failed to emit SubscriberJoin");
    }
    let reason = drive(&mut socket, &mut rx, &cancel).await;
    if let Err(err) = events.try_send(Event::SubscriberLeave) {
        tracing::debug!(%err, "failed to emit SubscriberLeave");
    }
    drop(events);

    let close = match reason {
        ExitReason::Lagged(n) => {
            tracing::warn!(dropped = n, "subscriber lagging, disconnecting");
            Some((CLOSE_CODE_POLICY, "lagged"))
        }
        ExitReason::Closed => {
            forward_final_snapshot(&mut socket, &mut final_snapshot, final_snapshot_wait).await;
            Some((CLOSE_CODE_NORMAL, "server shutdown"))
        }
        ExitReason::ClientGone | ExitReason::SendFailed => None,
    };
    if let Some((code, reason)) = close {
        let _ = socket
            .send(Message::Close(Some(CloseFrame {
                code,
                reason: std::borrow::Cow::Borrowed(reason),
            })))
            .await;
    }
    let _ = socket.close().await;
}

async fn forward_final_snapshot(
    socket: &mut WebSocket,
    final_snapshot: &mut watch::Receiver<Option<Arc<Snapshot>>>,
    final_snapshot_wait: Duration,
) {
    let initial_snapshot = final_snapshot.borrow().clone();
    let snap = if let Some(snap) = initial_snapshot {
        Some(snap)
    } else {
        let Ok(res) = timeout(final_snapshot_wait, final_snapshot.changed()).await else {
            return;
        };
        if res.is_err() {
            return;
        }
        final_snapshot.borrow().clone()
    };
    let Some(snap) = snap else { return };
    if let Ok(json) = serde_json::to_string(snap.as_ref()) {
        let _ = socket.send(Message::Text(json)).await;
    }
}

#[derive(Debug)]
enum ExitReason {
    Closed,
    ClientGone,
    Lagged(u64),
    SendFailed,
}

async fn drive(
    socket: &mut WebSocket,
    rx: &mut broadcast::Receiver<Arc<Snapshot>>,
    cancel: &CancellationToken,
) -> ExitReason {
    loop {
        tokio::select! {
            () = cancel.cancelled() => return ExitReason::Closed,
            frame = rx.recv() => match frame {
                Ok(snap) => {
                    let json = match serde_json::to_string(snap.as_ref()) {
                        Ok(j) => j,
                        Err(err) => {
                            tracing::error!(%err, "snapshot serialise failed");
                            continue;
                        }
                    };
                    if socket.send(Message::Text(json)).await.is_err() {
                        return ExitReason::SendFailed;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return ExitReason::Closed,
                Err(broadcast::error::RecvError::Lagged(n)) => return ExitReason::Lagged(n),
            },
            incoming = socket.recv() => match incoming {
                Some(Ok(Message::Close(_))) | None => return ExitReason::ClientGone,
                Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Text(_) | Message::Binary(_))) => {}
                Some(Err(err)) => {
                    tracing::debug!(%err, "subscriber read error");
                    return ExitReason::ClientGone;
                }
            }
        }
    }
}
