use std::time::Instant;

use flux_proto::{FluxPacket, MAX_PACKET_BYTES};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) enum Event {
    Packet(FluxPacket, Instant),
    ParseError,
    SubscriberJoin,
    SubscriberLeave,
}

pub(crate) async fn ingress_task(
    socket: UdpSocket,
    tx: mpsc::Sender<Event>,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let mut buf = [0u8; MAX_PACKET_BYTES];
    loop {
        tokio::select! {
            biased;
            () = cancel.cancelled() => return Ok(()),
            recv = socket.recv_from(&mut buf) => match recv {
                Ok((n, from)) => match FluxPacket::decode(&buf[..n]) {
                    Ok(packet) => {
                        if tx.send(Event::Packet(packet, Instant::now())).await.is_err() {
                            return Ok(());
                        }
                    }
                    Err(err) => {
                        tracing::debug!(%from, bytes = n, %err, "packet decode failed");
                        if tx.send(Event::ParseError).await.is_err() {
                            return Ok(());
                        }
                    }
                },
                Err(err) => {
                    tracing::warn!(%err, "UDP recv failed, continuing");
                }
            }
        }
    }
}
