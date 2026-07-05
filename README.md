# Oracle Verification Daemon (Rust)

The Oracle Verification Daemon continuously checks endpoint health for chains listed in the Cosm Registry smart contract.
It fetches active endpoints, probes them over multiple protocols, normalizes observations, and prepares publication batches.

## Project goals

- Keep endpoint health data fresh and machine-readable.
- Probe heterogeneous endpoint types with a single pipeline.
- Expose operational metrics for observability.
- Provide a safe fallback mode while on-chain publish messages are not yet available.

## Current behavior

1. Read configured chain targets from the CosmWasm registry contract through LCD smart queries.
2. Probe active endpoints (RPC, REST, gRPC, WebSocket) with bounded concurrency and jitter.
3. Build normalized observations with status and latency.
4. Batch observations by `chain_id`.
5. Push batches to a publisher abstraction.

## Degraded publish mode

The daemon currently uses `DegradedPublisher`.
Because the contract does not expose a submit message for observations yet, batches are queued in memory and logged.
This keeps probe and aggregation logic production-like while avoiding fake on-chain writes.

## Architecture overview

- `src/main.rs`: startup wiring (config, logging, metrics server, scheduler).
- `src/config.rs`: config loading, env overrides, validation.
- `src/scheduler.rs`: probe loop, concurrency control, batching, publish orchestration.
- `src/contract/queries.rs`: LCD smart query client and payload decoding.
- `src/probes/`: protocol-specific probe implementations and probe engine.
- `src/publisher.rs`: publisher trait and degraded in-memory publisher.
- `src/storage/`: backoff and queue storage utilities.
- `src/metrics.rs`: Prometheus registry and `/metrics` HTTP server.
- `src/models.rs`: shared domain models.

## Requirements

- Rust toolchain (stable).
- Network access to:
	- LCD endpoint configured in `oracle.toml`.
	- Probed chain endpoints.

## Quick start

1. Copy and edit the sample configuration:

```bash
cp oracle.example.toml oracle.toml
```

2. Start the daemon:

```bash
cargo run
```

3. Optional explicit config path:

```bash
ORACLE_CONFIG=oracle.toml cargo run
```

## Configuration

Base file: `oracle.example.toml`.

Main sections:

- `[oracle]`: contract source, intervals, timeouts, retries, batching, concurrency, jitter.
- `[probe.*]`: protocol toggles (`rpc`, `rest`, `grpc`, `websocket`).
- `[metrics]`: metrics enablement and bind address.
- `[logging]`: log level.

Environment overrides:

- `ORACLE_CONFIG`: config file path.
- `ORACLE_LOG_LEVEL`: log level override.
- `ORACLE_METRICS_ADDR`: metrics bind address override.

## Observability

If metrics are enabled, the daemon serves Prometheus metrics on:

- `GET /metrics`

Example (default config): `http://127.0.0.1:9090/metrics`

## Development commands

```bash
cargo check
cargo test
cargo run
```

## Known limitations

- No on-chain submission yet (degraded publisher only).
- Publication queue is in-memory and not persisted across restarts.
- Endpoint verification and ranking are outside current scope.

## Next milestones

- Add on-chain submission publisher when contract execute message is available.
- Persist publish queue for crash resilience.
- Add retry policies and dead-letter handling for failed publication flows.
