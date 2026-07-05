use std::str::FromStr;

use base64::Engine;
use bip39::{Language, Mnemonic};
use cosmrs::{
    cosmwasm::MsgExecuteContract,
    crypto::secp256k1,
    tx::{BodyBuilder, Fee, Msg, SignDoc, SignerInfo},
    AccountId, Coin,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::{
    config::AppConfig,
    error::OracleError,
    models::{EndpointObservation, EndpointStatus as ObservationStatus, SubmissionBatch},
    storage::queue::InMemoryQueue,
};

pub trait Publisher {
    async fn publish_batch(&mut self, batch: SubmissionBatch) -> Result<(), OracleError>;
}

#[derive(Default)]
pub struct DegradedPublisher {
    pub queue: InMemoryQueue,
}

impl Publisher for DegradedPublisher {
    async fn publish_batch(&mut self, batch: SubmissionBatch) -> Result<(), OracleError> {
        warn!(
            chain_id = %batch.chain_id,
            size = batch.observations.len(),
            "submit_endpoint_statuses unavailable on contract; queueing batch"
        );
        debug!(batch = ?batch, "queued batch");
        self.queue.push(batch);
        Ok(())
    }
}

pub struct SignedPublisher {
    cfg: AppConfig,
    http: Client,
    signer: secp256k1::SigningKey,
    sender: AccountId,
    contract: AccountId,
}

pub enum ActivePublisher {
    Signed(SignedPublisher),
    Degraded(DegradedPublisher),
}

impl Publisher for ActivePublisher {
    async fn publish_batch(&mut self, batch: SubmissionBatch) -> Result<(), OracleError> {
        match self {
            ActivePublisher::Signed(publisher) => publisher.publish_batch(batch).await,
            ActivePublisher::Degraded(publisher) => publisher.publish_batch(batch).await,
        }
    }
}

impl SignedPublisher {
    pub fn new(cfg: AppConfig) -> Result<Self, OracleError> {
        let mnemonic = cfg
            .oracle
            .wallet
            .mnemonic
            .clone()
            .ok_or_else(|| OracleError::Publisher("wallet mnemonic not configured".to_string()))?;

        let mnemonic = Mnemonic::parse_in(Language::English, mnemonic)
            .map_err(|e| OracleError::Publisher(format!("invalid wallet mnemonic: {e}")))?;
        let seed = mnemonic.to_seed_normalized("");
        let derivation_path = cfg
            .oracle
            .wallet
            .derivation_path
            .parse()
            .map_err(|e| OracleError::Publisher(format!("invalid derivation path: {e}")))?;
        let signer = secp256k1::SigningKey::derive_from_path(seed, &derivation_path)
            .map_err(|e| OracleError::Publisher(format!("cannot derive signing key: {e}")))?;
        let sender = signer
            .public_key()
            .account_id(&cfg.oracle.wallet.prefix)
            .map_err(|e| OracleError::Publisher(format!("cannot derive sender address: {e}")))?;
        let contract = AccountId::from_str(&cfg.oracle.contract_address)
            .map_err(|e| OracleError::Publisher(format!("invalid contract address: {e}")))?;
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(cfg.oracle.request_timeout_secs))
            .build()
            .map_err(|e| OracleError::Publisher(e.to_string()))?;

        Ok(Self {
            cfg,
            http,
            signer,
            sender,
            contract,
        })
    }

    async fn publish_once(&self, batch: &SubmissionBatch) -> Result<String, OracleError> {
        let (account_number, sequence) = self.fetch_account_state().await?;
        let execute_msg = OracleExecuteMsg::submit_endpoint_statuses(batch);
        let msg_bytes = serde_json::to_vec(&execute_msg)
            .map_err(|e| OracleError::Publisher(format!("cannot encode execute msg: {e}")))?;

        let msg = MsgExecuteContract {
            sender: self.sender.clone(),
            contract: self.contract.clone(),
            msg: msg_bytes,
            funds: vec![],
        }
        .to_any()
        .map_err(|e| OracleError::Publisher(format!("cannot encode contract msg: {e}")))?;

        let memo = self
            .cfg
            .oracle
            .wallet
            .memo
            .clone()
            .unwrap_or_default();
        let body = BodyBuilder::new().msg(msg).memo(memo).finish();
        
        let chain_id = self
            .cfg
            .oracle
            .chain_id
            .parse()
            .map_err(|e| OracleError::Publisher(format!("invalid chain id: {e}")))?;

        let signer_info = SignerInfo::single_direct(Some(self.signer.public_key()), sequence);
        let fee_denom = &self.cfg.oracle.wallet.fee_denom;

        // -------------------------------------------------------------
        // STEP 1: GAS SIMULATION
        // -------------------------------------------------------------
        let sim_fee_coin = Coin::new(self.cfg.oracle.wallet.fee_amount, fee_denom)
            .map_err(|e| OracleError::Publisher(format!("invalid fee coin for simulation: {e}")))?;
        let sim_fee = Fee::from_amount_and_gas(sim_fee_coin, self.cfg.oracle.wallet.gas_limit);
        let sim_auth_info = signer_info.clone().auth_info(sim_fee);
        
        let sim_sign_doc = SignDoc::new(&body, &sim_auth_info, &chain_id, account_number)
            .map_err(|e| OracleError::Publisher(format!("cannot build sim sign doc: {e}")))?;
        let sim_tx_raw = sim_sign_doc
            .sign(&self.signer)
            .map_err(|e| OracleError::Publisher(format!("cannot sign sim tx: {e}")))?;
        let sim_tx_bytes = sim_tx_raw
            .to_bytes()
            .map_err(|e| OracleError::Publisher(format!("cannot serialize sim tx: {e}")))?;

        let sim_response = self
            .http
            .post(format!(
                "{}/cosmos/tx/v1beta1/simulate",
                self.cfg.oracle.lcd_endpoint.trim_end_matches('/')
            ))
            .json(&SimulateTxRequest {
                tx_bytes: base64::engine::general_purpose::STANDARD.encode(sim_tx_bytes),
            })
            .send()
            .await
            .map_err(|e| OracleError::Publisher(format!("simulation request failed: {e}")))?;

        let sim_status = sim_response.status();
        let sim_body_text = sim_response
            .text()
            .await
            .map_err(|e| OracleError::Publisher(format!("cannot read simulation response: {e}")))?;

        if !sim_status.is_success() {
            return Err(OracleError::Publisher(format!(
                "simulation failed with status {}. Body: {}",
                sim_status, sim_body_text
            )));
        }

        let sim_json: serde_json::Value = serde_json::from_str(&sim_body_text)
            .map_err(|e| OracleError::Publisher(format!("invalid simulation JSON format: {e}")))?;

        // Resilient retrieval (handles whether the node returns a String or an Integer)
        let gas_used: u64 = match sim_json["gas_info"]["gas_used"].as_str() {
            Some(s) => s.parse().unwrap_or(0),
            None => sim_json["gas_info"]["gas_used"].as_u64().unwrap_or(0),
        };

        if gas_used == 0 {
            return Err(OracleError::Publisher(format!(
                "simulation returned 0 gas or could not be parsed: {}", sim_body_text
            )));
        }

        // Apply an adjustment factor of 1.4 (Gas Adjustment)
        let gas_adjustment = 1.4;
        let adjusted_gas = ((gas_used as f64) * gas_adjustment).ceil() as u64;

        // Calculate the cost of fees on a pro-rata basis based on the initially set gas price
        let base_gas_limit = self.cfg.oracle.wallet.gas_limit;
        let base_fee_amount = self.cfg.oracle.wallet.fee_amount;
        let adjusted_fee_amount = if base_gas_limit > 0 {
            (adjusted_gas as u128 * base_fee_amount) / base_gas_limit as u128
        } else {
            base_fee_amount
        };

        debug!(
            gas_used = gas_used,
            adjusted_gas = adjusted_gas,
            fee_amount = adjusted_fee_amount,
            "gas simulation completed successfully"
        );

        // -------------------------------------------------------------
        // STEP 2: FINAL SIGNATURE AND BROADCAST
        // -------------------------------------------------------------
        let final_fee_coin = Coin::new(adjusted_fee_amount, fee_denom)
            .map_err(|e| OracleError::Publisher(format!("invalid adjusted fee coin: {e}")))?;
        let final_fee = Fee::from_amount_and_gas(final_fee_coin, adjusted_gas);
        let auth_info = signer_info.auth_info(final_fee);

        let sign_doc = SignDoc::new(&body, &auth_info, &chain_id, account_number)
            .map_err(|e| OracleError::Publisher(format!("cannot build final sign doc: {e}")))?;
        let tx_raw = sign_doc
            .sign(&self.signer)
            .map_err(|e| OracleError::Publisher(format!("cannot sign final tx: {e}")))?;
        let tx_bytes = tx_raw
            .to_bytes()
            .map_err(|e| OracleError::Publisher(format!("cannot serialize final tx: {e}")))?;

        let response = self
            .http
            .post(format!(
                "{}/cosmos/tx/v1beta1/txs",
                self.cfg.oracle.lcd_endpoint.trim_end_matches('/')
            ))
            .json(&BroadcastTxRequest {
                tx_bytes: base64::engine::general_purpose::STANDARD.encode(tx_bytes),
                mode: "BROADCAST_MODE_SYNC".to_string(),
            })
            .send()
            .await
            .map_err(|e| OracleError::Publisher(format!("broadcast request failed: {e}")))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|e| OracleError::Publisher(format!("cannot read broadcast response: {e}")))?;
        if !status.is_success() {
            return Err(OracleError::Publisher(format!(
                "broadcast failed with status {}. Body starts with: {}",
                status,
                body_text.chars().take(160).collect::<String>()
            )));
        }

        let response: BroadcastTxResponse = serde_json::from_str(&body_text)
            .map_err(|e| OracleError::Publisher(format!("invalid broadcast response: {e}")))?;
        let tx_response = response
            .tx_response
            .ok_or_else(|| OracleError::Publisher("missing tx_response field".to_string()))?;
        if tx_response.code != 0 {
            return Err(OracleError::Publisher(format!(
                "tx rejected code={} raw_log={}",
                tx_response.code, tx_response.raw_log
            )));
        }

        Ok(tx_response.txhash)
    }

    async fn fetch_account_state(&self) -> Result<(u64, u64), OracleError> {
        let url = format!(
            "{}/cosmos/auth/v1beta1/accounts/{}",
            self.cfg.oracle.lcd_endpoint.trim_end_matches('/'),
            self.sender
        );
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| OracleError::Publisher(format!("account query failed: {e}")))?;

        let status = response.status();
        let body_text = response
            .text()
            .await
            .map_err(|e| OracleError::Publisher(format!("cannot read account response: {e}")))?;
        if !status.is_success() {
            return Err(OracleError::Publisher(format!(
                "account query failed with status {}. Body starts with: {}",
                status,
                body_text.chars().take(160).collect::<String>()
            )));
        }

        let response: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| OracleError::Publisher(format!("invalid account response: {e}")))?;
        let account = response
            .get("account")
            .ok_or_else(|| OracleError::Publisher("missing account field".to_string()))?;

        let account_number = if let Some(base) = account.get("base_account") {
            base.get("account_number")
        } else {
            account.get("account_number")
        }
        .and_then(|v| v.as_str())
        .ok_or_else(|| OracleError::Publisher("missing account_number field".to_string()))?
        .parse()
        .map_err(|e| OracleError::Publisher(format!("invalid account number: {e}")))?;

        let sequence = if let Some(base) = account.get("base_account") {
            base.get("sequence")
        } else {
            account.get("sequence")
        }
        .and_then(|v| v.as_str())
        .ok_or_else(|| OracleError::Publisher("missing sequence field".to_string()))?
        .parse()
        .map_err(|e| OracleError::Publisher(format!("invalid account sequence: {e}")))?;

        Ok((account_number, sequence))
    }
}

impl Publisher for SignedPublisher {
    async fn publish_batch(&mut self, batch: SubmissionBatch) -> Result<(), OracleError> {
        let mut last_error = None;
        for attempt in 0..=self.cfg.oracle.max_retries {
            match self.publish_once(&batch).await {
                Ok(txhash) => {
                    info!(chain_id = %batch.chain_id, txhash = %txhash, "submit_endpoint_statuses broadcasted");
                    return Ok(());
                }
                Err(err) => {
                    last_error = Some(err.to_string());
                    if attempt < self.cfg.oracle.max_retries {
                        warn!(attempt = attempt + 1, "publication failed, retrying");
                    }
                }
            }
        }

        Err(OracleError::Publisher(format!(
            "publication failed after retries: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        )))
    }
}

#[derive(Debug, Serialize)]
struct SimulateTxRequest {
    #[serde(rename = "tx_bytes")]
    tx_bytes: String,
}

#[derive(Debug, Serialize)]
struct BroadcastTxRequest {
    #[serde(rename = "tx_bytes")]
    tx_bytes: String,
    mode: String,
}

#[derive(Debug, Deserialize)]
struct BroadcastTxResponse {
    #[serde(rename = "tx_response")]
    tx_response: Option<TxResponse>,
}

#[derive(Debug, Deserialize)]
struct TxResponse {
    code: u32,
    txhash: String,
    raw_log: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct OracleExecuteMsg {
    submit_endpoint_statuses: SubmitEndpointStatuses,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct SubmitEndpointStatuses {
    chain_id: String,
    observations: Vec<EndpointObservationInput>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct EndpointObservationInput {
    endpoint_id: u64,
    status: ObservationStatus,
    latency_ms: Option<u32>,
}

impl OracleExecuteMsg {
    fn submit_endpoint_statuses(batch: &SubmissionBatch) -> Self {
        Self {
            submit_endpoint_statuses: SubmitEndpointStatuses {
                chain_id: batch.chain_id.clone(),
                observations: batch
                    .observations
                    .iter()
                    .map(EndpointObservationInput::from)
                    .collect(),
            },
        }
    }
}

impl From<&EndpointObservation> for EndpointObservationInput {
    fn from(value: &EndpointObservation) -> Self {
        // Security/Sanitization: An offline endpoint must NEVER send latency to the contract.
        let sanitized_latency = match value.status {
            ObservationStatus::Online => value.latency_ms,
            ObservationStatus::Offline => None, 
        };

        Self {
            endpoint_id: value.endpoint_id,
            status: value.status,
            latency_ms: sanitized_latency,
        }
    }
}
