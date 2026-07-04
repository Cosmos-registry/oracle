use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct EndpointView {
    pub endpoint_id: u64,
    pub chain_id: String,
    pub kind: EndpointKind,
    pub url: String,
    pub active: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointKind {
    Rpc,
    Rest,
    Grpc,
    Wss,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EndpointsResponse {
    pub endpoints: Vec<EndpointView>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainListItem {
    pub chain_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainsResponse {
    pub chains: Vec<ChainListItem>,
}
