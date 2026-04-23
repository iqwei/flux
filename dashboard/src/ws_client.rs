use std::sync::Arc;
use std::time::Duration;

use flux_proto::Snapshot;
use futures_util::StreamExt;
use rand::Rng;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use tokio_util::sync::CancellationToken;

use crate::app::AppEvent;
use crate::config::ReconnectPolicy;

const BACKOFF_SHIFT_CAP: u32 = 10;

type Stream = WebSocketStream<MaybeTlsStream<TcpStream>>;

pub async fn run_ws(
    url: String,
    policy: ReconnectPolicy,
    tx: mpsc::Sender<AppEvent>,
    cancel: CancellationToken,
) {
    let mut attempt: u32 = 0;
    loop {
        if cancel.is_cancelled() {
            return;
        }
        match connect(&url, &cancel).await {
            Connect::Established(stream) => {
                attempt = 0;
                if tx.send(AppEvent::Connected).await.is_err() {
                    return;
                }
                pump(stream, &tx, &cancel).await;
                if cancel.is_cancelled() {
                    return;
                }
                if tx.send(AppEvent::Disconnected).await.is_err() {
                    return;
                }
            }
            Connect::Failed(err) => {
                tracing::debug!(%err, url = %url, "ws connect failed");
                if cancel.is_cancelled() {
                    return;
                }
                if tx.send(AppEvent::Disconnected).await.is_err() {
                    return;
                }
            }
            Connect::Cancelled => return,
        }

        attempt = attempt.saturating_add(1);
        let delay = backoff(&policy, attempt);
        let in_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX);
        if tx
            .send(AppEvent::Reconnecting { attempt, in_ms })
            .await
            .is_err()
        {
            return;
        }
        tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            () = sleep(delay) => {}
        }
    }
}

enum Connect {
    Established(Stream),
    Failed(tokio_tungstenite::tungstenite::Error),
    Cancelled,
}

async fn connect(url: &str, cancel: &CancellationToken) -> Connect {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Connect::Cancelled,
        res = connect_async(url) => match res {
            Ok((stream, _)) => Connect::Established(stream),
            Err(err) => Connect::Failed(err),
        }
    }
}

async fn pump(mut stream: Stream, tx: &mpsc::Sender<AppEvent>, cancel: &CancellationToken) {
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return,
            frame = stream.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str::<Snapshot>(&text) {
                        Ok(snap) => {
                            if tx.send(AppEvent::Snapshot(Arc::new(snap))).await.is_err() {
                                return;
                            }
                        }
                        Err(err) => tracing::debug!(%err, "invalid snapshot payload"),
                    }
                }
                Some(Ok(
                    Message::Binary(_)
                    | Message::Ping(_)
                    | Message::Pong(_)
                    | Message::Frame(_),
                )) => {}
                Some(Ok(Message::Close(_))) | None => return,
                Some(Err(err)) => {
                    tracing::debug!(%err, "ws recv error");
                    return;
                }
            }
        }
    }
}

fn backoff(policy: &ReconnectPolicy, attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(BACKOFF_SHIFT_CAP);
    let factor = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
    let base = policy.initial_ms.saturating_mul(factor).min(policy.max_ms);
    if policy.jitter <= 0.0 {
        return Duration::from_millis(base);
    }
    let jitter = policy.jitter.clamp(0.0, 1.0);
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    let spread = ((base as f64) * jitter) as u64;
    if spread == 0 {
        return Duration::from_millis(base);
    }
    let mut rng = rand::thread_rng();
    let offset = rng.gen_range(0..=spread);
    let millis = base.saturating_sub(spread / 2).saturating_add(offset);
    Duration::from_millis(millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(initial: u64, max: u64, jitter: f64) -> ReconnectPolicy {
        ReconnectPolicy {
            initial_ms: initial,
            max_ms: max,
            jitter,
        }
    }

    #[test]
    fn backoff_grows_then_caps() {
        let p = policy(100, 1_000, 0.0);
        assert_eq!(backoff(&p, 1), Duration::from_millis(100));
        assert_eq!(backoff(&p, 2), Duration::from_millis(200));
        assert_eq!(backoff(&p, 3), Duration::from_millis(400));
        assert_eq!(backoff(&p, 4), Duration::from_millis(800));
        assert_eq!(backoff(&p, 5), Duration::from_secs(1));
        assert_eq!(backoff(&p, 100), Duration::from_secs(1));
    }

    #[test]
    fn backoff_with_zero_jitter_is_deterministic() {
        let p = policy(250, 5_000, 0.0);
        assert_eq!(backoff(&p, 1), Duration::from_millis(250));
        assert_eq!(backoff(&p, 10), Duration::from_secs(5));
    }

    #[test]
    fn backoff_with_jitter_stays_in_bounds() {
        let p = policy(100, 1_000, 0.2);
        for attempt in 1..=20u32 {
            let shift = attempt.saturating_sub(1).min(BACKOFF_SHIFT_CAP);
            let factor = 1u64.checked_shl(shift).unwrap_or(u64::MAX);
            let base = 100u64.saturating_mul(factor).min(1_000);
            #[allow(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss
            )]
            let spread = ((base as f64) * 0.2) as u64;
            let lo = base.saturating_sub(spread / 2);
            let hi = base.saturating_add(spread);
            let ms = u64::try_from(backoff(&p, attempt).as_millis()).unwrap();
            assert!(
                ms >= lo && ms <= hi,
                "attempt={attempt} ms={ms} base={base}"
            );
        }
    }
}
