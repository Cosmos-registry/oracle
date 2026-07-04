use thiserror::Error;

#[derive(Debug, Error)]
pub enum OracleError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("contract source error: {0}")]
    ContractSource(String),

    #[error("probe failed: {0}")]
    Probe(String),

    #[error("publisher error: {0}")]
    Publisher(String),
}
