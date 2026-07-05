use std::time::Instant;

use reqwest::Client;

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointObservation, EndpointStatus, ProbeTarget},
};

pub async fn probe_rest(
    http: &Client,
    cfg: &AppConfig,
    target: &ProbeTarget,
    now: u64,
) -> Result<EndpointObservation, OracleError> {
    let start = Instant::now();
    let base = target.url.trim_end_matches('/');
    let node_info_url = format!("{base}/cosmos/base/tendermint/v1beta1/node_info");
    let status_url = format!("{base}/cosmos/base/node/v1beta1/status");

    let node_res = http
        .get(node_info_url)
        .timeout(std::time::Duration::from_secs(cfg.oracle.request_timeout_secs))
        .send()
        .await
        .map_err(|e| OracleError::Probe(e.to_string()))?;

    let status_res = http
        .get(status_url)
        .timeout(std::time::Duration::from_secs(cfg.oracle.request_timeout_secs))
        .send()
        .await
        .map_err(|e| OracleError::Probe(e.to_string()))?;

    let node_json: serde_json::Value = node_res
        .json()
        .await
        .map_err(|e| OracleError::Probe(e.to_string()))?;
    let status_json: serde_json::Value = status_res
        .json()
        .await
        .map_err(|e| OracleError::Probe(e.to_string()))?;

    let network_ok = node_json
        .pointer("/default_node_info/network")
        .and_then(|v| v.as_str())
        .map(|s| s == target.chain_id)
        .unwrap_or(false);

    // let has_timestamp = status_json
    //     .pointer("/status/sync_info/latest_block_time")
    //     .or_else(|| status_json.pointer("/sync_info/latest_block_time"))
    //     .is_some();

    // let online = network_ok && has_timestamp;
    let online = network_ok;
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
