use std::sync::Arc;

use axum::{Router, extract::State, routing::get};
use prometheus::{Encoder, HistogramVec, IntCounter, IntGauge, Registry, TextEncoder};

use crate::error::OracleError;

#[derive(Clone)]
pub struct Metrics {
    pub registry: Registry,
    pub probes_total: IntCounter,
    pub probe_success_total: IntCounter,
    pub probe_failure_total: IntCounter,
    pub tx_success_total: IntCounter,
    pub tx_failure_total: IntCounter,
    pub queue_depth: IntGauge,
    pub probe_latency_ms: HistogramVec,
}

impl Metrics {
    pub fn new() -> Result<Self, OracleError> {
        let registry = Registry::new();

        let probes_total = IntCounter::new("oracle_probes_total", "Total number of probes")
            .map_err(|e| OracleError::Config(e.to_string()))?;
        let probe_success_total = IntCounter::new(
            "oracle_probe_success_total",
            "Total successful probes",
        )
        .map_err(|e| OracleError::Config(e.to_string()))?;
        let probe_failure_total = IntCounter::new(
            "oracle_probe_failure_total",
            "Total failed probes",
        )
        .map_err(|e| OracleError::Config(e.to_string()))?;
        let tx_success_total =
            IntCounter::new("oracle_tx_success_total", "Total publication successes")
                .map_err(|e| OracleError::Config(e.to_string()))?;
        let tx_failure_total =
            IntCounter::new("oracle_tx_failure_total", "Total publication failures")
                .map_err(|e| OracleError::Config(e.to_string()))?;
        let queue_depth = IntGauge::new("oracle_queue_depth", "Queued publication batches")
            .map_err(|e| OracleError::Config(e.to_string()))?;
        let probe_latency_ms = HistogramVec::new(
            prometheus::HistogramOpts::new("oracle_probe_latency_ms", "Probe latency in ms"),
            &["protocol"],
        )
        .map_err(|e| OracleError::Config(e.to_string()))?;

        registry
            .register(Box::new(probes_total.clone()))
            .map_err(|e| OracleError::Config(e.to_string()))?;
        registry
            .register(Box::new(probe_success_total.clone()))
            .map_err(|e| OracleError::Config(e.to_string()))?;
        registry
            .register(Box::new(probe_failure_total.clone()))
            .map_err(|e| OracleError::Config(e.to_string()))?;
        registry
            .register(Box::new(tx_success_total.clone()))
            .map_err(|e| OracleError::Config(e.to_string()))?;
        registry
            .register(Box::new(tx_failure_total.clone()))
            .map_err(|e| OracleError::Config(e.to_string()))?;
        registry
            .register(Box::new(queue_depth.clone()))
            .map_err(|e| OracleError::Config(e.to_string()))?;
        registry
            .register(Box::new(probe_latency_ms.clone()))
            .map_err(|e| OracleError::Config(e.to_string()))?;

        Ok(Self {
            registry,
            probes_total,
            probe_success_total,
            probe_failure_total,
            tx_success_total,
            tx_failure_total,
            queue_depth,
            probe_latency_ms,
        })
    }

    pub async fn serve(self, addr: &str) -> Result<(), OracleError> {
        let state = Arc::new(self);
        let app = Router::new()
            .route("/metrics", get(metrics_handler))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| OracleError::Config(format!("cannot bind metrics addr {addr}: {e}")))?;

        axum::serve(listener, app)
            .await
            .map_err(|e| OracleError::Config(format!("metrics server failed: {e}")))
    }
}

async fn metrics_handler(State(metrics): State<Arc<Metrics>>) -> Result<String, String> {
    let metric_families = metrics.registry.gather();
    let mut buffer = Vec::new();
    let encoder = TextEncoder::new();

    encoder
        .encode(&metric_families, &mut buffer)
        .map_err(|e| format!("encode metrics failed: {e}"))?;

    String::from_utf8(buffer).map_err(|e| format!("utf8 metrics failed: {e}"))
}
