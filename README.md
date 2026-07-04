# Oracle Verification Daemon (Rust)

MVP implementation aligned with the oracle verification specification.

## Current scope

- Full daemon scaffold with modular architecture.
- Contract refresh at each cycle via smart queries.
- Multi-protocol probe engine: RPC, REST, WebSocket, gRPC.
- Observation normalization with online/offline status and latency.
- Batching by chain id.
- Publisher abstraction with degraded in-memory queue mode.
- Prometheus metrics endpoint.

## Why degraded publish mode

The current Cosm-registry contract does not yet expose `SubmitEndpointStatuses`.
The daemon therefore keeps batches in memory and logs publication intent.

## Run

1. Copy the sample config.
2. Adapt values for your environment.
3. Start the daemon.

```bash
cp oracle.example.toml oracle.toml
cargo run
```

## Config

A sample config file is provided at `oracle.example.toml`.

You can override some values with environment variables:

- `ORACLE_CONFIG`
- `ORACLE_LOG_LEVEL`
- `ORACLE_METRICS_ADDR`

## Build checks

```bash
cargo check
```
