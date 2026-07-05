use base64::Engine;
use reqwest::Client;
use serde_json::json;
use urlencoding::encode;

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointType, ProbeTarget},
};

use super::messages::{ChainsResponse, EndpointKind, EndpointsResponse};

#[derive(Clone)]
pub struct HttpContractSource {
    http: Client,
    lcd_endpoint: String,
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
            lcd_endpoint: cfg.oracle.lcd_endpoint.clone(),
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

        let query_data = serde_json::to_vec(&json!({
            "get_chains": {"start_after": null, "limit": 100}
        }))
        .map_err(|e| OracleError::ContractSource(e.to_string()))?;
            let query_data_b64 = base64::engine::general_purpose::STANDARD.encode(query_data);
            let query_data_path = encode(&query_data_b64);
        let endpoint = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            self.lcd_endpoint.trim_end_matches('/'),
            self.contract_address,
            query_data_path
        );

        let res = self
            .http
            .get(endpoint)
            .send()
            .await
            .map_err(|e| OracleError::ContractSource(e.to_string()))?;

        let status = res.status();
        let body_text = res
            .text()
            .await
            .map_err(|e| OracleError::ContractSource(format!("cannot read response body: {e}")))?;
        if !status.is_success() {
            return Err(OracleError::ContractSource(format!(
                "LCD smart query failed with status {}. Body starts with: {}",
                status,
                body_text.chars().take(160).collect::<String>()
            )));
        }

        let body: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| OracleError::ContractSource(format!("invalid JSON response: {e}")))?;

        let parsed: ChainsResponse = parse_data_payload(&body, "get_chains")?;
        Ok(parsed.chains.into_iter().map(|c| c.chain_id).collect())
    }

    async fn fetch_chain_endpoints(&self, chain_id: &str) -> Result<EndpointsResponse, OracleError> {
        let query_data = serde_json::to_vec(&json!({
            "get_endpoints": {
                "chain_id": chain_id,
                "kind": null,
                "include_inactive": false
            }
        }))
        .map_err(|e| OracleError::ContractSource(e.to_string()))?;
            let query_data_b64 = base64::engine::general_purpose::STANDARD.encode(query_data);
            let query_data_path = encode(&query_data_b64);
        let endpoint = format!(
            "{}/cosmwasm/wasm/v1/contract/{}/smart/{}",
            self.lcd_endpoint.trim_end_matches('/'),
            self.contract_address,
            query_data_path
        );

        let res = self
            .http
            .get(endpoint)
            .send()
            .await
            .map_err(|e| OracleError::ContractSource(e.to_string()))?;

        let status = res.status();
        let body_text = res
            .text()
            .await
            .map_err(|e| OracleError::ContractSource(format!("cannot read response body: {e}")))?;
        if !status.is_success() {
            return Err(OracleError::ContractSource(format!(
                "LCD smart query failed with status {}. Body starts with: {}",
                status,
                body_text.chars().take(160).collect::<String>()
            )));
        }

        let body: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| OracleError::ContractSource(format!("invalid JSON response: {e}")))?;

        let parsed: EndpointsResponse = parse_data_payload(&body, "get_endpoints")?;
        Ok(parsed)
    }
}

fn parse_data_payload<T>(body: &serde_json::Value, query_name: &str) -> Result<T, OracleError>
where
    T: serde::de::DeserializeOwned,
{
    let data_value = body.get("data").ok_or_else(|| {
        OracleError::ContractSource(format!("missing data field in {query_name} response"))
    })?;

    if data_value.is_object() || data_value.is_array() {
        return serde_json::from_value(data_value.clone()).map_err(|e| {
            OracleError::ContractSource(format!("invalid {query_name} JSON object payload: {e}"))
        });
    }

    let data = data_value.as_str().ok_or_else(|| {
        OracleError::ContractSource(format!(
            "unsupported data field type in {query_name} response"
        ))
    })?;

    if data.trim_start().starts_with('{') || data.trim_start().starts_with('[') {
        return serde_json::from_str(data).map_err(|e| {
            OracleError::ContractSource(format!("invalid {query_name} JSON string payload: {e}"))
        });
    }

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| OracleError::ContractSource(format!("invalid base64 in {query_name}: {e}")))?;
    serde_json::from_slice(&decoded)
        .map_err(|e| OracleError::ContractSource(format!("invalid {query_name} payload: {e}")))
}
