use reqwest::Client;
use serde_json::json;
use base64::Engine;

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointType, ProbeTarget},
};

use super::messages::{ChainsResponse, EndpointKind, EndpointsResponse};

#[derive(Clone)]
pub struct HttpContractSource {
    http: Client,
    grpc_endpoint: String,
    contract_address: String,
}

impl HttpContractSource {
    pub fn new(cfg: &AppConfig) -> Result<Self, OracleError> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(cfg.oracle.request_timeout_secs))
            .build()
            .map_err(|e| OracleError::ContractSource(e.to_string()))?;

        Ok(Self {
            http,
            grpc_endpoint: cfg.oracle.grpc_endpoint.clone(),
            contract_address: cfg.oracle.contract_address.clone(),
        })
    }

    pub async fn fetch_targets(&self) -> Result<Vec<ProbeTarget>, OracleError> {
        let chain_ids = self.fetch_chain_ids().await?;
        let mut targets = Vec::new();

        for chain_id in chain_ids {
            let endpoints = self.fetch_chain_endpoints(&chain_id).await?;
            for endpoint in endpoints.endpoints.into_iter().filter(|e| e.active) {
                targets.push(ProbeTarget {
                    chain_id: endpoint.chain_id,
                    endpoint_id: endpoint.endpoint_id,
                    endpoint_type: match endpoint.kind {
                        EndpointKind::Rpc => EndpointType::Rpc,
                        EndpointKind::Rest => EndpointType::Rest,
                        EndpointKind::Grpc => EndpointType::Grpc,
                        EndpointKind::Wss => EndpointType::Websocket,
                    },
                    url: endpoint.url,
                });
            }
        }

        Ok(targets)
    }

    async fn fetch_chain_ids(&self) -> Result<Vec<String>, OracleError> {
        if self.contract_address.is_empty() {
            return Ok(Vec::new());
        }

        let endpoint = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            self.grpc_endpoint.trim_end_matches('/'),
            self.contract_address,
            hex::encode(serde_json::to_vec(&json!({"get_chains": {"start_after": null, "limit": 100}})).map_err(|e| OracleError::ContractSource(e.to_string()))?)
        );

        let res = self
            .http
            .get(endpoint)
            .send()
            .await
            .map_err(|e| OracleError::ContractSource(e.to_string()))?;

        let body: serde_json::Value = res
            .json()
            .await
            .map_err(|e| OracleError::ContractSource(e.to_string()))?;

        let data = body
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OracleError::ContractSource("missing base64 data in get_chains".to_string()))?;

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| OracleError::ContractSource(format!("invalid base64: {e}")))?;
        let parsed: ChainsResponse = serde_json::from_slice(&decoded)
            .map_err(|e| OracleError::ContractSource(format!("invalid get_chains payload: {e}")))?;
        Ok(parsed.chains.into_iter().map(|c| c.chain_id).collect())
    }

    async fn fetch_chain_endpoints(&self, chain_id: &str) -> Result<EndpointsResponse, OracleError> {
        let endpoint = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            self.grpc_endpoint.trim_end_matches('/'),
            self.contract_address,
            hex::encode(serde_json::to_vec(&json!({
                "get_endpoints": {
                    "chain_id": chain_id,
                    "kind": null,
                    "include_inactive": false
                }
            }))
            .map_err(|e| OracleError::ContractSource(e.to_string()))?)
        );

        let res = self
            .http
            .get(endpoint)
            .send()
            .await
            .map_err(|e| OracleError::ContractSource(e.to_string()))?;

        let body: serde_json::Value = res
            .json()
            .await
            .map_err(|e| OracleError::ContractSource(e.to_string()))?;

        let data = body
            .get("data")
            .and_then(|v| v.as_str())
            .ok_or_else(|| OracleError::ContractSource("missing base64 data in get_endpoints".to_string()))?;

        let decoded = base64::engine::general_purpose::STANDARD
            .decode(data)
            .map_err(|e| OracleError::ContractSource(format!("invalid base64: {e}")))?;
        let parsed: EndpointsResponse = serde_json::from_slice(&decoded)
            .map_err(|e| OracleError::ContractSource(format!("invalid get_endpoints payload: {e}")))?;
        Ok(parsed)
    }
}
