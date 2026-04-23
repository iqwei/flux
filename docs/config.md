# Flux Configuration Reference

Flux has three configuration surfaces:

- `flux-server`
- `flux-producer`
- `flux-dashboard`

All config files are TOML. All three binaries reject unknown keys. Partial files
are valid because missing keys fall back to the binary defaults.

## Precedence

`flux-server`

1. built-in defaults
2. config file passed with `--config`
3. `FLUX_*` environment variables
4. CLI port overrides

`flux-producer`

1. built-in defaults
2. config file passed with `--config`
3. `--target`

`flux-dashboard`

1. built-in defaults
2. config file passed with `--config`
3. `--server`

## Server

Example:

```toml
udp_bind = "0.0.0.0:9000"
ws_bind = "0.0.0.0:9001"
broadcast_interval_ms = 500
ingress_buffer = 4096
broadcast_buffer = 16
rolling_window_ms = 5000
stale_after_ms = 3000
dead_after_ms = 10000
shutdown_budget_ms = 500
```

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `udp_bind` | `SocketAddr` | `0.0.0.0:9000` | UDP bind address for binary ingest. |
| `ws_bind` | `SocketAddr` | `0.0.0.0:9001` | TCP bind address for the WebSocket server. |
| `broadcast_interval_ms` | `u64` | `500` | Snapshot broadcast cadence in milliseconds. Must be `> 0`. |
| `ingress_buffer` | `usize` | `4096` | Capacity of the ingress `mpsc` channel from UDP ingest to the aggregator. Must be `> 0`. |
| `broadcast_buffer` | `usize` | `16` | Capacity of the broadcast channel that fans out snapshots to WebSocket subscribers. Must be `> 0`. |
| `rolling_window_ms` | `u64` | `5000` | Length of the rolling aggregation window used for `min`, `max`, `avg`, and `rate_pps`. Must be `> 0`. |
| `stale_after_ms` | `u64` | `3000` | Age threshold the dashboard uses to dim a metric row. Must be `< dead_after_ms`. |
| `dead_after_ms` | `u64` | `10000` | Age threshold the dashboard uses to mark a metric as dead. Must be `> stale_after_ms`. |
| `shutdown_budget_ms` | `u64` | `500` | Upper bound for ingress drain and final snapshot fanout during shutdown. Must be `> 0`. |

### Server environment variables

| Environment variable | Maps to |
| --- | --- |
| `FLUX_UDP_BIND` | `udp_bind` |
| `FLUX_WS_BIND` | `ws_bind` |
| `FLUX_BROADCAST_INTERVAL_MS` | `broadcast_interval_ms` |
| `FLUX_INGRESS_BUFFER` | `ingress_buffer` |
| `FLUX_BROADCAST_BUFFER` | `broadcast_buffer` |
| `FLUX_ROLLING_WINDOW_MS` | `rolling_window_ms` |
| `FLUX_STALE_AFTER_MS` | `stale_after_ms` |
| `FLUX_DEAD_AFTER_MS` | `dead_after_ms` |
| `FLUX_SHUTDOWN_BUDGET_MS` | `shutdown_budget_ms` |

### Server CLI

| Flag | Meaning |
| --- | --- |
| `--config PATH` | Load TOML config from `PATH`. |
| `--udp-port PORT` | Override the port portion of `udp_bind`. |
| `--ws-port PORT` | Override the port portion of `ws_bind`. |
| `--log FILTER` | Override the tracing filter directly. |

`flux-server` honors `RUST_LOG`. If `--log` is present, it wins over
`RUST_LOG`.

## Producer

Example:

```toml
target = "127.0.0.1:9000"

[[metrics]]
name = "cpu.load"
unit = "percent"
rate_hz = 5.0
simulation = { kind = "sine", amplitude = 25.0, period_ms = 4000, offset = 50.0 }

[[metrics]]
name = "memory.used_mb"
unit = "MB"
rate_hz = 2.0
simulation = { kind = "random_range", min = 2048.0, max = 8192.0 }
```

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `target` | `SocketAddr` | `127.0.0.1:9000` | UDP target for emitted packets. |
| `metrics` | `Vec<MetricSpec>` | built-in demo metrics | Metric emitters to spawn. Must not be empty after config loading completes. |

### Producer metric fields

| Key | Type | Required | Meaning |
| --- | --- | --- | --- |
| `name` | `String` | yes | Metric name sent over the wire. Must be non-empty and at most 64 bytes. |
| `unit` | `Option<String>` | no | Unit label stored in config. Current behavior: the wire format does not carry unit metadata yet, so dashboard snapshots still show `unit = null`. |
| `rate_hz` | `f64` | yes | Emit rate for this metric. Must be finite and `> 0`. |
| `simulation` | inline table | yes | Simulator definition for this metric. |

### Producer simulators

| `simulation.kind` | Fields | Meaning |
| --- | --- | --- |
| `constant` | `value: f64` | Emits the same floating-point value every tick. |
| `random_range` | `min: f64`, `max: f64` | Emits a uniform random `f64` in `[min, max]`. |
| `sine` | `amplitude: f64`, `period_ms: u64`, `offset: f64` | Emits `offset + amplitude * sin(2*pi*t/period_ms)`. |
| `stepped` | `values: Vec<f64>`, `dwell_ms: u64` | Cycles through `values`, holding each value for `dwell_ms`. |

The current producer always emits `F64` packet values, even though
`FluxPacket` supports `U64`, `I64`, `F64`, and `Bool`.

### Built-in producer defaults

Running `cargo run -p flux-producer` with no config file creates these demo
metrics:

| Name | Rate | Simulation |
| --- | --- | --- |
| `cpu.load` | `5.0 Hz` | sine, amplitude `25.0`, period `4000 ms`, offset `50.0` |
| `memory.used_mb` | `2.0 Hz` | random range, `2048.0..=8192.0` |
| `sensor.online` | `1.0 Hz` | stepped, values `[1.0, 1.0, 0.0, 1.0]`, dwell `1500 ms` |

### Producer CLI

| Flag | Meaning |
| --- | --- |
| `--config PATH` | Load TOML config from `PATH`. |
| `--target ADDR` | Override the final UDP target. |
| `--log FILTER` | Override the tracing filter directly. |

`flux-producer` honors `RUST_LOG`. If `--log` is present, it wins over
`RUST_LOG`.

## Dashboard

Example:

```toml
server_url = "ws://127.0.0.1:9001/ws"

[reconnect]
initial_ms = 500
max_ms = 10000
jitter = 0.2

[thresholds]
stale_after_ms = 3000
dead_after_ms = 10000
```

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `server_url` | `String` | `ws://127.0.0.1:9001/ws` | WebSocket endpoint to subscribe to. |
| `reconnect.initial_ms` | `u64` | `500` | First reconnect delay. Must be `> 0`. |
| `reconnect.max_ms` | `u64` | `10000` | Maximum reconnect delay. Must be `>= reconnect.initial_ms`. |
| `reconnect.jitter` | `f64` | `0.2` | Randomization factor in `[0.0, 1.0]` applied to reconnect sleeps. |
| `thresholds.stale_after_ms` | `u64` | `3000` | Age threshold for stale rows. Must be `> 0`. |
| `thresholds.dead_after_ms` | `u64` | `10000` | Age threshold for dead rows. Must be `> thresholds.stale_after_ms`. |

### Dashboard CLI

| Flag | Meaning |
| --- | --- |
| `--config PATH` | Load TOML config from `PATH`. |
| `--server URL` | Override `server_url`. |
| `--log-file PATH` | Write tracing logs to `PATH`. |

The dashboard only initializes tracing when `--log-file` is present. When it is
present, the subscriber honors `RUST_LOG`; otherwise dashboard logs stay off.

## Related Docs

- [Packet format specification](../spec/packet.md)
- [Workspace quickstart](../README.md)
- [Example producer config](../config/producer.example.toml)
