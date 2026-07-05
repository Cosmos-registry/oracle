pub mod grpc;
pub mod rest;
pub mod rpc;
pub mod ws;
use tracing::{debug};
use reqwest::Client;

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointObservation, EndpointType, ProbeTarget},
};

#[derive(Clone)]
pub struct ProbeEngine {
    http: Client,
    cfg: AppConfig,
}

impl ProbeEngine {
    pub fn new(cfg: AppConfig) -> Result<Self, OracleError> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(cfg.oracle.request_timeout_secs))
            .build()
            .map_err(|e| OracleError::Probe(e.to_string()))?;
        Ok(Self { http, cfg })
    }

    pub async fn probe(&self, target: &ProbeTarget, now: u64) -> Result<EndpointObservation, OracleError> {
        debug!("probing endpoint: {} ({:?})", target.url, target.endpoint_type);
        match target.endpoint_type {
            EndpointType::Rpc if self.cfg.probe.rpc.enabled => {
                rpc::probe_rpc(&self.http, &self.cfg, target, now).await
            }
            EndpointType::Rest if self.cfg.probe.rest.enabled => {
                rest::probe_rest(&self.http, &self.cfg, target, now).await
            }
            EndpointType::Grpc if self.cfg.probe.grpc.enabled => {
                grpc::probe_grpc(&self.cfg, target, now).await
            }
            EndpointType::Websocket if self.cfg.probe.websocket.enabled => {
                ws::probe_ws(&self.cfg, target, now).await
            }
            _ => Err(OracleError::Probe("probe type disabled by config".to_string())),
        }
    }
}
