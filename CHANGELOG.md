# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project uses semantic versioning for tagged releases.

## [0.1.0] - 2026-04-23

### Added

- `flux-proto` with the binary packet format, snapshot types, and protocol
  tests.
- `flux-server` with UDP ingest, rolling-window aggregation, pipeline health
  tracking, WebSocket fanout, and graceful shutdown.
- `flux-producer` with TOML-driven metric simulation and concurrent UDP emitters.
- `flux-dashboard` with auto-reconnecting WebSocket subscription, staleness
  signaling, filtering, and terminal-safe shutdown.
- Packet format documentation, configuration reference, quickstart, and demo
  tooling.
