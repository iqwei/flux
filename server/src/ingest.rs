use std::io::ErrorKind;
use std::net::SocketAddr;
use std::time::Duration;
use std::time::Instant;

use flux_proto::{FluxPacket, MAX_PACKET_BYTES};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) enum Event {
    Packet(FluxPacket, Instant),
    ParseError,
    SubscriberJoin,
    SubscriberLeave,
}

const MAX_SHUTDOWN_IDLE_WAIT: Duration = Duration::from_millis(25);

pub(crate) async fn ingress_task(
    socket: UdpSocket,
    tx: mpsc::Sender<Event>,
    shutdown_budget: Duration,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let mut buf = [0u8; MAX_PACKET_BYTES];
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => {
                drain_shutdown_datagrams(&socket, &tx, &mut buf, shutdown_budget).await;
                return Ok(());
            }
            recv = socket.recv_from(&mut buf) => match recv {
                Ok((n, from)) => {
                    if !forward_datagram(&tx, &buf[..n], from).await {
                        return Ok(());
                    }
                }
                Err(err) => {
                    tracing::warn!(%err, "UDP recv failed, continuing");
                }
            }
        }
    }
}

async fn drain_shutdown_datagrams(
    socket: &UdpSocket,
    tx: &mpsc::Sender<Event>,
    buf: &mut [u8; MAX_PACKET_BYTES],
    shutdown_budget: Duration,
) {
    let idle_wait = shutdown_budget.min(MAX_SHUTDOWN_IDLE_WAIT);
    loop {
        drain_ready_datagrams(socket, tx, buf).await;
        let recv = timeout(idle_wait, socket.recv_from(buf)).await;
        match recv {
            Ok(Ok((n, from))) => {
                if !forward_datagram(tx, &buf[..n], from).await {
                    return;
                }
            }
            Ok(Err(err)) => {
                tracing::warn!(%err, "UDP recv failed while waiting for shutdown drain");
                return;
            }
            Err(_) => return,
        }
    }
}

async fn drain_ready_datagrams(
    socket: &UdpSocket,
    tx: &mpsc::Sender<Event>,
    buf: &mut [u8; MAX_PACKET_BYTES],
) {
    loop {
        match socket.try_recv_from(buf) {
            Ok((n, from)) => {
                if !forward_datagram(tx, &buf[..n], from).await {
                    return;
                }
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => return,
            Err(err) => {
                tracing::warn!(%err, "UDP recv failed while draining shutdown buffer");
                return;
            }
        }
    }
}

async fn forward_datagram(
    tx: &mpsc::Sender<Event>,
    bytes: &[u8],
    from: SocketAddr,
) -> bool {
    match FluxPacket::decode(bytes) {
        Ok(packet) => tx.send(Event::Packet(packet, Instant::now())).await.is_ok(),
        Err(err) => {
            tracing::debug!(%from, bytes = bytes.len(), %err, "packet decode failed");
            tx.send(Event::ParseError).await.is_ok()
        }
    }
}
