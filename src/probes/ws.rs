use std::time::Instant;

use futures_util::{SinkExt, StreamExt};
use tokio::time::{Duration, timeout};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointObservation, EndpointStatus, ProbeTarget},
};

pub async fn probe_ws(
    cfg: &AppConfig,
    target: &ProbeTarget,
    now: u64,
) -> Result<EndpointObservation, OracleError> {
    let start = Instant::now();
    let ws_url = target.url.clone();

    let (mut socket, _) = timeout(
        Duration::from_secs(cfg.oracle.ws_timeout_secs),
        connect_async(&ws_url),
    )
    .await
    .map_err(|_| OracleError::Probe("ws connect timeout".to_string()))?
    .map_err(|e| OracleError::Probe(e.to_string()))?;

    socket
        .send(Message::Ping(vec![1]))
        .await
        .map_err(|e| OracleError::Probe(e.to_string()))?;

    let recv_ok = timeout(Duration::from_secs(cfg.oracle.ws_timeout_secs), socket.next())
        .await
        .map_err(|_| OracleError::Probe("ws recv timeout".to_string()))?
        .is_some();

    let elapsed_ms = start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;

    Ok(EndpointObservation {
        chain_id: target.chain_id.clone(),
        endpoint_id: target.endpoint_id,
        endpoint_type: target.endpoint_type,
        status: if recv_ok {
            EndpointStatus::Online
        } else {
            EndpointStatus::Offline
        },
        latency_ms: Some(elapsed_ms),
        checked_at: now,
    })
}
