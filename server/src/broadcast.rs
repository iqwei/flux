use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use flux_proto::Snapshot;
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc};
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
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let router = Router::new()
        .route("/ws", get(ws_handler))
        .with_state(state);
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { cancel.cancelled().await })
        .await?;
    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<WsState>>) -> Response {
    ws.on_upgrade(move |socket| run_subscriber(socket, state))
}

async fn run_subscriber(mut socket: WebSocket, state: Arc<WsState>) {
    let mut rx = state.snapshots.subscribe();
    let _ = state.events.send(Event::SubscriberJoin).await;
    let reason = drive(&mut socket, &mut rx).await;
    match reason {
        ExitReason::Lagged(n) => {
            tracing::warn!(dropped = n, "subscriber lagging, disconnecting");
        }
        ExitReason::Closed | ExitReason::ClientGone | ExitReason::SendFailed => {}
    }
    let _ = state.events.send(Event::SubscriberLeave).await;
    let _ = socket.close().await;
}

#[derive(Debug)]
enum ExitReason {
    Closed,
    ClientGone,
    Lagged(u64),
    SendFailed,
}

async fn drive(socket: &mut WebSocket, rx: &mut broadcast::Receiver<Arc<Snapshot>>) -> ExitReason {
    loop {
        tokio::select! {
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
