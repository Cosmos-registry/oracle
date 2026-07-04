mod config;
mod contract;
mod error;
mod metrics;
mod models;
mod probes;
mod publisher;
mod scheduler;
mod storage;

use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

use crate::{
    config::AppConfig,
    contract::queries::HttpContractSource,
    error::OracleError,
    metrics::Metrics,
    probes::ProbeEngine,
    publisher::DegradedPublisher,
    scheduler::Scheduler,
};

#[tokio::main]
async fn main() -> Result<(), OracleError> {
    let cfg = AppConfig::load()?;

    let filter =
        EnvFilter::try_new(cfg.logging.level.clone()).unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).with_target(false).init();

    let metrics = Metrics::new()?;
    if cfg.metrics.enabled {
        let metrics_clone = metrics.clone();
        let addr = cfg.metrics.addr.clone();
        tokio::spawn(async move {
            if let Err(e) = metrics_clone.serve(&addr).await {
                tracing::error!(error = %e, "metrics server failed");
            }
        });
    }

    info!(
        chain_id = %cfg.oracle.chain_id,
        contract_address = %cfg.oracle.contract_address,
        "oracle daemon starting"
    );

    let source = HttpContractSource::new(&cfg)?;
    let probe_engine = ProbeEngine::new(cfg.clone())?;
    let publisher = DegradedPublisher::default();

    let scheduler = Scheduler::new(cfg, source, probe_engine, publisher, metrics);
    scheduler.run().await;

    Ok(())
}
