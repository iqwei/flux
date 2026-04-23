#![allow(clippy::unwrap_used)]

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use flux_proto::{FluxPacket, Snapshot, ValueKind, MAX_PACKET_BYTES};
use flux_server::{bind, ServerConfig};
use futures_util::StreamExt;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Instant};
use tokio_tungstenite::tungstenite::protocol::Message;
use tokio_util::sync::CancellationToken;

fn ephemeral_cfg() -> ServerConfig {
    ServerConfig {
        udp_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        ws_bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        broadcast_interval_ms: 50,
        ..ServerConfig::default()
    }
}

#[tokio::test]
async fn pipeline_delivers_metric_to_websocket_subscriber() {
    let cancel = CancellationToken::new();
    let server = bind(ephemeral_cfg(), cancel.clone()).await.unwrap();
    let udp_addr = server.udp_addr;
    let ws_url = format!("ws://{}/ws", server.ws_addr);
    let wait_handle = tokio::spawn(server.wait());

    let (mut ws, _resp) = timeout(
        Duration::from_secs(2),
        tokio_tungstenite::connect_async(&ws_url),
    )
    .await
    .unwrap()
    .unwrap();

    let producer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let packet = FluxPacket::new(1_700_000_000_000, "cpu.temp".into(), ValueKind::F64(42.5));
    let mut buf = [0u8; MAX_PACKET_BYTES];
    let n = packet.encode(&mut buf).unwrap();

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut matched = false;
    while Instant::now() < deadline && !matched {
        producer.send_to(&buf[..n], udp_addr).await.unwrap();
        let Ok(Some(Ok(msg))) = timeout(Duration::from_millis(500), ws.next()).await else {
            continue;
        };
        let Message::Text(text) = msg else { continue };
        let snap: Snapshot = serde_json::from_str(&text).unwrap();
        if snap
            .metrics
            .iter()
            .any(|m| m.name == "cpu.temp" && m.last == Some(42.5))
        {
            matched = true;
        }
    }
    assert!(
        matched,
        "expected cpu.temp metric to arrive on WS within 3s"
    );

    let _ = ws.close(None).await;
    cancel.cancel();
    timeout(Duration::from_secs(3), wait_handle)
        .await
        .expect("server did not shut down within 3s")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn cancellation_shuts_down_cleanly_with_no_traffic() {
    let cancel = CancellationToken::new();
    let server = bind(ephemeral_cfg(), cancel.clone()).await.unwrap();
    let wait_handle = tokio::spawn(server.wait());
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();
    timeout(Duration::from_secs(3), wait_handle)
        .await
        .expect("server did not shut down within 3s")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn shutdown_under_traffic_stays_within_budget_and_closes_ws() {
    let cfg = ServerConfig {
        shutdown_budget_ms: 750,
        broadcast_buffer: 2,
        broadcast_interval_ms: 20,
        ingress_buffer: 1,
        ..ephemeral_cfg()
    };
    let cancel = CancellationToken::new();
    let server = bind(cfg, cancel.clone()).await.unwrap();
    let udp_addr = server.udp_addr;
    let ws_url = format!("ws://{}/ws", server.ws_addr);
    let wait_handle = tokio::spawn(server.wait());

    let (mut ws, _) = timeout(
        Duration::from_secs(2),
        tokio_tungstenite::connect_async(&ws_url),
    )
    .await
    .unwrap()
    .unwrap();

    let producer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for _ in 0..20 {
        send_packet(&producer, udp_addr, 1.0).await;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    send_packet(&producer, udp_addr, 99.0).await;

    let start = Instant::now();
    cancel.cancel();

    let mut saw_close = false;
    let mut last_snapshot: Option<Snapshot> = None;
    while let Ok(Some(frame)) = timeout(Duration::from_millis(500), ws.next()).await {
        match frame {
            Ok(Message::Text(text)) => {
                last_snapshot = Some(serde_json::from_str(&text).unwrap());
            }
            Ok(Message::Close(_)) => {
                saw_close = true;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }

    timeout(Duration::from_secs(2), wait_handle)
        .await
        .expect("server did not shut down within 2s")
        .unwrap()
        .unwrap();

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(1_500),
        "shutdown took {elapsed:?}, expected < 1500ms"
    );
    let last_snapshot = last_snapshot.expect("expected a final snapshot before close");
    let load = last_snapshot
        .metrics
        .iter()
        .find(|metric| metric.name == "load")
        .and_then(|metric| metric.last);
    assert_eq!(
        load,
        Some(99.0),
        "expected final snapshot to carry the last packet sent before shutdown"
    );
    assert!(saw_close, "expected WS Close frame during shutdown");
}

async fn send_packet(sock: &UdpSocket, target: SocketAddr, value: f64) {
    let packet = FluxPacket::new(1_700_000_000_000, "load".into(), ValueKind::F64(value));
    let mut buf = [0u8; MAX_PACKET_BYTES];
    let n = packet.encode(&mut buf).unwrap();
    sock.send_to(&buf[..n], target).await.unwrap();
}
