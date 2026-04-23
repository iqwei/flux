#![allow(clippy::unwrap_used)]

use flux_proto::{
    DecodeError, EncodeError, FluxPacket, ValueKind, HEADER_BYTES, MAX_NAME_LEN, MAX_PACKET_BYTES,
    PROTOCOL_VERSION, TAG_BOOL, TAG_F64, TAG_I64, TAG_U64,
};
use proptest::prelude::*;

fn encode_to_vec(packet: &FluxPacket) -> Vec<u8> {
    let mut buf = vec![0u8; packet.encoded_len()];
    let n = packet.encode(&mut buf).unwrap();
    buf.truncate(n);
    buf
}

fn expect_truncated(bytes: &[u8], context: &str) {
    match FluxPacket::decode(bytes) {
        Err(DecodeError::Truncated) => {}
        other => panic!("{context}: expected Truncated, got {other:?}"),
    }
}

#[test]
fn roundtrip_u64() {
    let packet = FluxPacket::new(
        0x1122_3344_5566_7788,
        "cpu.ticks".into(),
        ValueKind::U64(42),
    );
    let bytes = encode_to_vec(&packet);
    assert_eq!(bytes[0], PROTOCOL_VERSION);
    assert_eq!(bytes[1..9], 0x1122_3344_5566_7788u64.to_be_bytes());
    assert_eq!(bytes[9] as usize, "cpu.ticks".len());
    assert_eq!(bytes[HEADER_BYTES + "cpu.ticks".len()], TAG_U64);
    let decoded = FluxPacket::decode(&bytes).unwrap();
    assert_eq!(decoded, packet);
}

#[test]
fn roundtrip_i64() {
    let packet = FluxPacket::new(1, "delta".into(), ValueKind::I64(-123_456));
    let bytes = encode_to_vec(&packet);
    assert_eq!(bytes[HEADER_BYTES + "delta".len()], TAG_I64);
    assert_eq!(FluxPacket::decode(&bytes).unwrap(), packet);
}

#[test]
fn roundtrip_f64() {
    let packet = FluxPacket::new(2, "voltage".into(), ValueKind::F64(-12_345.678_9));
    let bytes = encode_to_vec(&packet);
    assert_eq!(bytes[HEADER_BYTES + "voltage".len()], TAG_F64);
    assert_eq!(FluxPacket::decode(&bytes).unwrap(), packet);
}

#[test]
fn roundtrip_bool() {
    for flag in [false, true] {
        let packet = FluxPacket::new(3, "live".into(), ValueKind::Bool(flag));
        let bytes = encode_to_vec(&packet);
        assert_eq!(bytes[HEADER_BYTES + "live".len()], TAG_BOOL);
        assert_eq!(bytes[HEADER_BYTES + "live".len() + 1], u8::from(flag));
        assert_eq!(FluxPacket::decode(&bytes).unwrap(), packet);
    }
}

#[test]
fn encoded_len_matches_encode() {
    let packet = FluxPacket::new(7, "m".into(), ValueKind::F64(1.0));
    let mut buf = [0u8; MAX_PACKET_BYTES];
    let n = packet.encode(&mut buf).unwrap();
    assert_eq!(n, packet.encoded_len());
}

#[test]
fn encode_buffer_too_small_returns_error() {
    let packet = FluxPacket::new(0, "x".into(), ValueKind::U64(1));
    let mut buf = [0u8; 4];
    assert_eq!(packet.encode(&mut buf), Err(EncodeError::BufferTooSmall));
}

#[test]
fn encode_rejects_name_longer_than_max() {
    let name = "a".repeat(MAX_NAME_LEN + 1);
    let packet = FluxPacket::new(0, name, ValueKind::Bool(false));
    let mut buf = [0u8; MAX_PACKET_BYTES];
    assert_eq!(packet.encode(&mut buf), Err(EncodeError::NameTooLong));
}

#[test]
fn name_length_boundaries_roundtrip() {
    for len in [0usize, 1, MAX_NAME_LEN] {
        let name = "n".repeat(len);
        let packet = FluxPacket::new(0, name.clone(), ValueKind::U64(0));
        let bytes = encode_to_vec(&packet);
        let decoded = FluxPacket::decode(&bytes).unwrap();
        assert_eq!(decoded.name.len(), len);
        assert_eq!(decoded, packet);
    }
}

#[test]
fn decode_rejects_name_longer_than_max() {
    let mut buf = vec![0u8; HEADER_BYTES + MAX_NAME_LEN + 2];
    buf[0] = PROTOCOL_VERSION;
    buf[9] = u8::try_from(MAX_NAME_LEN + 1).unwrap();
    match FluxPacket::decode(&buf) {
        Err(DecodeError::NameTooLong) => {}
        other => panic!("expected NameTooLong, got {other:?}"),
    }
}

#[test]
fn decode_rejects_unknown_version() {
    let packet = FluxPacket::new(0, "x".into(), ValueKind::U64(1));
    let mut bytes = encode_to_vec(&packet);
    bytes[0] = 0x02;
    match FluxPacket::decode(&bytes) {
        Err(DecodeError::UnknownVersion(v)) => assert_eq!(v, 0x02),
        other => panic!("expected UnknownVersion, got {other:?}"),
    }
}

#[test]
fn decode_rejects_unknown_value_type() {
    let packet = FluxPacket::new(0, "x".into(), ValueKind::U64(1));
    let mut bytes = encode_to_vec(&packet);
    let tag_offset = HEADER_BYTES + "x".len();
    bytes[tag_offset] = 0xFF;
    match FluxPacket::decode(&bytes) {
        Err(DecodeError::UnknownValueType(v)) => assert_eq!(v, 0xFF),
        other => panic!("expected UnknownValueType, got {other:?}"),
    }
}

#[test]
fn decode_rejects_invalid_bool_byte() {
    let packet = FluxPacket::new(0, "b".into(), ValueKind::Bool(true));
    let mut bytes = encode_to_vec(&packet);
    let value_offset = HEADER_BYTES + "b".len() + 1;
    bytes[value_offset] = 0x02;
    match FluxPacket::decode(&bytes) {
        Err(DecodeError::InvalidBoolByte(v)) => assert_eq!(v, 0x02),
        other => panic!("expected InvalidBoolByte, got {other:?}"),
    }
}

#[test]
fn decode_rejects_non_utf8_name() {
    let packet = FluxPacket::new(0, "xx".into(), ValueKind::U64(1));
    let mut bytes = encode_to_vec(&packet);
    bytes[HEADER_BYTES] = 0xFF;
    bytes[HEADER_BYTES + 1] = 0xFE;
    match FluxPacket::decode(&bytes) {
        Err(DecodeError::NameNotUtf8) => {}
        other => panic!("expected NameNotUtf8, got {other:?}"),
    }
}

#[test]
fn decode_rejects_trailing_bytes() {
    let packet = FluxPacket::new(0, "x".into(), ValueKind::U64(1));
    let mut bytes = encode_to_vec(&packet);
    bytes.push(0x00);
    match FluxPacket::decode(&bytes) {
        Err(DecodeError::TrailingBytes) => {}
        other => panic!("expected TrailingBytes, got {other:?}"),
    }
}

#[test]
fn decode_truncated_at_every_prefix_length() {
    let variants = [
        ValueKind::U64(0xDEAD_BEEF_CAFE_BABE),
        ValueKind::I64(-1),
        ValueKind::F64(1_234.567_89),
        ValueKind::Bool(true),
    ];
    for value in variants {
        let packet = FluxPacket::new(1_700_000_000_000, "metric.name".into(), value);
        let bytes = encode_to_vec(&packet);
        for prefix in 0..bytes.len() {
            expect_truncated(&bytes[..prefix], &format!("{value:?} prefix {prefix}"));
        }
        assert_eq!(FluxPacket::decode(&bytes).unwrap(), packet);
    }
}

#[test]
fn nan_roundtrip_preserves_bits() {
    let nan = f64::from_bits(0x7ff8_0000_0000_0001);
    let packet = FluxPacket::new(1, "nan".into(), ValueKind::F64(nan));
    let bytes = encode_to_vec(&packet);
    let decoded = FluxPacket::decode(&bytes).unwrap();
    match decoded.value {
        ValueKind::F64(v) => assert_eq!(v.to_bits(), nan.to_bits()),
        other => panic!("expected f64, got {other:?}"),
    }
}

#[test]
fn infinity_roundtrip() {
    for v in [f64::INFINITY, f64::NEG_INFINITY] {
        let packet = FluxPacket::new(0, "inf".into(), ValueKind::F64(v));
        let bytes = encode_to_vec(&packet);
        assert_eq!(FluxPacket::decode(&bytes).unwrap(), packet);
    }
}

fn value_kind_strategy() -> impl Strategy<Value = ValueKind> {
    prop_oneof![
        any::<u64>().prop_map(ValueKind::U64),
        any::<i64>().prop_map(ValueKind::I64),
        any::<f64>()
            .prop_filter("no nan", |v| !v.is_nan())
            .prop_map(ValueKind::F64),
        any::<bool>().prop_map(ValueKind::Bool),
    ]
}

fn packet_strategy() -> impl Strategy<Value = FluxPacket> {
    ("[a-zA-Z0-9._-]{0,64}", any::<u64>(), value_kind_strategy())
        .prop_map(|(name, ts, value)| FluxPacket::new(ts, name, value))
}

proptest! {
    #[test]
    fn proptest_roundtrip(packet in packet_strategy()) {
        let mut buf = vec![0u8; packet.encoded_len()];
        let n = packet.encode(&mut buf).unwrap();
        let decoded = FluxPacket::decode(&buf[..n]).unwrap();
        prop_assert_eq!(decoded, packet);
    }

    #[test]
    fn proptest_decode_random_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..=MAX_PACKET_BYTES)) {
        let _ = FluxPacket::decode(&bytes);
    }
}
