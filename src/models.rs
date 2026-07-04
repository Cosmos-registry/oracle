use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointType {
    Rpc,
    Rest,
    Grpc,
    Websocket,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointStatus {
    Online,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeTarget {
    pub chain_id: String,
    pub endpoint_id: u64,
    pub endpoint_type: EndpointType,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndpointObservation {
    pub chain_id: String,
    pub endpoint_id: u64,
    pub endpoint_type: EndpointType,
    pub status: EndpointStatus,
    pub latency_ms: Option<u32>,
    pub checked_at: u64,
}

#[derive(Debug, Clone)]
pub struct SubmissionBatch {
    pub chain_id: String,
    pub observations: Vec<EndpointObservation>,
}
