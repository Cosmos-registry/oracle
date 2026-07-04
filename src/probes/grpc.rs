use std::time::Instant;

use tonic::transport::Endpoint;

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointObservation, EndpointStatus, ProbeTarget},
};

pub async fn probe_grpc(
    cfg: &AppConfig,
    target: &ProbeTarget,
    now: u64,
) -> Result<EndpointObservation, OracleError> {
    let start = Instant::now();

    let endpoint = Endpoint::from_shared(target.url.clone())
        .map_err(|e| OracleError::Probe(format!("invalid grpc endpoint: {e}")))?
        .connect_timeout(std::time::Duration::from_secs(cfg.oracle.grpc_timeout_secs))
        .timeout(std::time::Duration::from_secs(cfg.oracle.grpc_timeout_secs));

    let status = match endpoint.connect().await {
        Ok(_) => EndpointStatus::Online,
        Err(_) => EndpointStatus::Offline,
    };

    let elapsed_ms = start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;

    Ok(EndpointObservation {
        chain_id: target.chain_id.clone(),
        endpoint_id: target.endpoint_id,
        endpoint_type: target.endpoint_type,
        status,
        latency_ms: Some(elapsed_ms),
        checked_at: now,
    })
}
