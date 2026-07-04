use std::time::Instant;

use reqwest::Client;

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointObservation, EndpointStatus, ProbeTarget},
};

pub async fn probe_rpc(
    http: &Client,
    cfg: &AppConfig,
    target: &ProbeTarget,
    now: u64,
) -> Result<EndpointObservation, OracleError> {
    let start = Instant::now();
    let status_url = format!("{}/status", target.url.trim_end_matches('/'));

    let res = http
        .get(status_url)
        .timeout(std::time::Duration::from_secs(cfg.oracle.request_timeout_secs))
        .send()
        .await
        .map_err(|e| OracleError::Probe(e.to_string()))?;

    let json: serde_json::Value = res
        .json()
        .await
        .map_err(|e| OracleError::Probe(e.to_string()))?;

    let network_ok = json
        .pointer("/result/node_info/network")
        .and_then(|v| v.as_str())
        .map(|s| s == target.chain_id)
        .unwrap_or(false);

    let catching_up = json
        .pointer("/result/sync_info/catching_up")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let online = network_ok && !catching_up;
    let elapsed_ms = start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;

    Ok(EndpointObservation {
        chain_id: target.chain_id.clone(),
        endpoint_id: target.endpoint_id,
        endpoint_type: target.endpoint_type,
        status: if online {
            EndpointStatus::Online
        } else {
            EndpointStatus::Offline
        },
        latency_ms: Some(elapsed_ms),
        checked_at: now,
    })
}
