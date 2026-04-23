# Flux Packet Format

Version: `0x01`. Status: stable within `0.x`.

## Contents

- [Overview](#overview)
- [Byte layout](#byte-layout)
- [Fields](#fields)
- [Value encodings](#value-encodings)
- [Endianness](#endianness)
- [Versioning](#versioning)
- [Size budget](#size-budget)
- [Worked examples](#worked-examples)
- [Rationale](#rationale)

## Overview

Flux packets carry a single metric sample from a producer to the server over
UDP. Each datagram is exactly one packet. The format is fixed-layout,
big-endian, and self-delimiting: the decoder can reject any malformed or
truncated input without allocating or executing unsafe code.

Reference implementation: [`flux-proto`](../proto) crate
(`FluxPacket::encode` / `FluxPacket::decode`).

Related docs:

- [Workspace quickstart](../README.md)
- [Configuration reference](../docs/config.md)

This document covers the producer-to-server UDP hop only. The server-to-client
hop is a JSON `Snapshot` stream over WebSocket at `/ws`.

## Byte layout

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  ver  |                   timestamp_ms (u64, BE)              |
+-------+                                                       |
|                                                               |
+-------+-------+-----------------------------------------------+
| n_len |           name (n_len bytes, UTF-8, 0..=64)           |
+-------+-----------+-------------------------------------------+
|   tag |     value (length depends on tag, see below)          |
+-------+-----------+-------------------------------------------+
```

Fixed header is 10 bytes: `ver (1) | timestamp_ms (8) | name_len (1)`.
Body is `name_len` bytes of UTF-8, then a 1-byte value `tag`, then the
tag-specific value payload.

## Fields

| Offset       | Size    | Name           | Type      | Description                                     |
|--------------|---------|----------------|-----------|-------------------------------------------------|
| `0`          | `1`     | `version`      | `u8`      | Protocol version. Current: `0x01`.              |
| `1`          | `8`     | `timestamp_ms` | `u64` BE  | Producer-side timestamp, ms since Unix epoch.   |
| `9`          | `1`     | `name_len`     | `u8`      | Length of `name` in bytes. `0..=64`.            |
| `10`         | `N`     | `name`         | UTF-8     | Metric name. Must decode as valid UTF-8.        |
| `10 + N`     | `1`     | `tag`          | `u8`      | Value type tag (`0x00..=0x03`).                 |
| `11 + N`     | `V`     | `value`        | see below | Tag-specific payload.                           |

`N = name_len`. `V` depends on `tag`.

## Value encodings

| Tag    | Name   | Payload size | Encoding                                           |
|--------|--------|--------------|----------------------------------------------------|
| `0x00` | `U64`  | 8            | Big-endian `u64`.                                  |
| `0x01` | `I64`  | 8            | Big-endian two's-complement `i64`.                 |
| `0x02` | `F64`  | 8            | Big-endian IEEE-754 binary64 (bit-exact roundtrip).|
| `0x03` | `Bool` | 1            | `0x00` = false, `0x01` = true. Other bytes reject. |

`NaN` and `±Infinity` are preserved bit-for-bit by `F64`. Aggregators decide
whether to include them in statistics.

## Endianness

All multi-byte scalars are **big-endian** (network byte order). This applies
to `timestamp_ms`, `U64`, `I64`, and `F64` payloads.

## Versioning

- The server reads `version` first and rejects any value other than the
  version it was compiled for with `DecodeError::UnknownVersion(v)`.
- Rejected packets are counted in `PipelineHealth.packets_parse_err` but do
  not terminate the ingress task.
- Forward compatibility: new versions get a new byte. Old servers drop
  newer packets and keep running. Upgrade the server first, then producers.
- Non-breaking additions (new `tag` values) are signalled by a new
  `version`. Unknown tags within the current version are always a hard
  error (`DecodeError::UnknownValueType(t)`).

## Size budget

| Component      | Bytes            |
|----------------|------------------|
| Header         | `10`             |
| Name (max)     | `64`             |
| Value tag      | `1`              |
| Value (max)    | `8`              |
| **Worst case** | `83`             |
| `MAX_PACKET_BYTES` guardrail | `512` |

The 83-byte worst case sits comfortably below the typical non-fragmented
UDP MTU (~1200 B on the public internet, ~1472 B on a sane LAN), so a
Flux packet never fragments on any reasonable path. The `512` guardrail
is the size buffer callers should allocate for receive — it is deliberately
generous so future format extensions stay wire-compatible without retuning
buffers.

## Worked examples

Bytes are shown in hexadecimal, grouped by field.

### 1. `U64` — `"cpu.ticks" = 42` at `timestamp_ms = 5`

```
01                                  ver
00 00 00 00 00 00 00 05             timestamp_ms
09                                  name_len
63 70 75 2e 74 69 63 6b 73          name ("cpu.ticks")
00                                  tag U64
00 00 00 00 00 00 00 2a             value (42)
```

Total: 28 bytes.

### 2. `F64` — `"volts" = 1.5` at `timestamp_ms = 1`

```
01                                  ver
00 00 00 00 00 00 00 01             timestamp_ms
05                                  name_len
76 6f 6c 74 73                      name ("volts")
02                                  tag F64
3f f8 00 00 00 00 00 00             value (1.5 in IEEE-754 BE)
```

Total: 24 bytes.

### 3. `Bool` — `"live" = true` at `timestamp_ms = 2`

```
01                                  ver
00 00 00 00 00 00 00 02             timestamp_ms
04                                  name_len
6c 69 76 65                         name ("live")
03                                  tag Bool
01                                  value (true)
```

Total: 16 bytes.

## Rationale

### Why not JSON?

A JSON-encoded sample of the same shape is typically 60–120 bytes with
quoted keys, ASCII-decimal numbers, and whitespace. A `u64` at its maximum
value takes 20 ASCII bytes versus 8 on the wire. At a few thousand samples
per second per producer the difference is measurable in both bandwidth and
allocator pressure. JSON also leaves parser behavior under-specified for
edge cases (numbers outside `i53`, trailing whitespace, duplicate keys),
which works against a format whose primary consumer is a low-latency
aggregator.

### Why not `serde` + a binary codec (bincode, postcard, ...)?

Those codecs are excellent for crate-internal types, but they couple the
wire format to the Rust struct layout and to the codec's versioning model.
A hand-rolled fixed layout lets any language implement a producer from
this document alone, and gives us a single source of truth for
compatibility rules (`UnknownVersion`, `UnknownValueType`, truncation
semantics) without reading through a dependency's changelog.

### Why big-endian?

Network byte order. It's the default every wire-level tool (`tcpdump`,
`wireshark`, `xxd`) expects when displaying protocol fields, so a hex dump
of a Flux packet reads left-to-right without a mental byte-swap.

### Why a 64-byte name cap?

A `u8` length prefix is the smallest self-delimiting encoding. 64 bytes is
long enough for `service.component.submetric` style names and short enough
that the worst-case packet stays well under typical MTU. The cap is
enforced both on encode and decode, and violations are distinguishable
errors (`NameTooLong`).
