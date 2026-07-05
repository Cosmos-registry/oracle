use std::{env, fs, path::Path};

use serde::{Deserialize, Serialize};

use crate::error::OracleError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub oracle: OracleConfig,
    pub probe: ProbeConfig,
    pub metrics: MetricsConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OracleConfig {
    pub chain_id: String,
    pub contract_address: String,
    #[serde(alias = "grpc_endpoint")]
    pub lcd_endpoint: String,
    #[serde(default)]
    pub wallet: WalletConfig,
    pub probe_interval_secs: u64,
    pub publish_interval_secs: u64,
    pub batch_size: usize,
    pub max_retries: u8,
    pub request_timeout_secs: u64,
    pub ws_timeout_secs: u64,
    pub grpc_timeout_secs: u64,
    pub max_clock_jitter_secs: u64,
    pub jitter_pct: u8,
    pub max_concurrency: usize,
}

impl Default for WalletConfig {
    fn default() -> Self {
        Self {
            mnemonic: None,
            prefix: "cosmos".to_string(),
            derivation_path: "m/44'/118'/0'/0/0".to_string(),
            fee_denom: "uatom".to_string(),
            fee_amount: 1_500,
            gas_limit: 250_000,
            memo: Some("oracle verification".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletConfig {
    pub mnemonic: Option<String>,
    pub prefix: String,
    pub derivation_path: String,
    pub fee_denom: String,
    pub fee_amount: u128,
    pub gas_limit: u64,
    pub memo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeConfig {
    pub rpc: ProbeToggle,
    pub rest: ProbeToggle,
    pub grpc: ProbeToggle,
    pub websocket: ProbeToggle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeToggle {
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            oracle: OracleConfig {
                chain_id: "localnet".to_string(),
                contract_address: "".to_string(),
                lcd_endpoint: "http://127.0.0.1:1317".to_string(),
                wallet: WalletConfig::default(),
                probe_interval_secs: 60,
                publish_interval_secs: 30,
                batch_size: 100,
                max_retries: 2,
                request_timeout_secs: 10,
                ws_timeout_secs: 10,
                grpc_timeout_secs: 10,
                max_clock_jitter_secs: 30,
                jitter_pct: 15,
                max_concurrency: 64,
            },
            probe: ProbeConfig {
                rpc: ProbeToggle { enabled: true },
                rest: ProbeToggle { enabled: true },
                grpc: ProbeToggle { enabled: true },
                websocket: ProbeToggle { enabled: true },
            },
            metrics: MetricsConfig {
                enabled: true,
                addr: "127.0.0.1:9090".to_string(),
            },
            logging: LoggingConfig {
                level: "info".to_string(),
            },
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, OracleError> {
        let config_path = env::var("ORACLE_CONFIG").unwrap_or_else(|_| "oracle.toml".to_string());

        let mut cfg = if Path::new(&config_path).exists() {
            let raw = fs::read_to_string(&config_path)
                .map_err(|e| OracleError::Config(format!("cannot read {}: {e}", config_path)))?;
            toml::from_str::<AppConfig>(&raw)
                .map_err(|e| OracleError::Config(format!("cannot parse {}: {e}", config_path)))?
        } else {
            AppConfig::default()
        };

        if let Ok(value) = env::var("ORACLE_LOG_LEVEL") {
            cfg.logging.level = value;
        }
        if let Ok(value) = env::var("ORACLE_METRICS_ADDR") {
            cfg.metrics.addr = value;
        }
        if let Ok(value) = env::var("ORACLE_WALLET_MNEMONIC") {
            cfg.oracle.wallet.mnemonic = Some(value);
        }
        if let Ok(value) = env::var("ORACLE_WALLET_PREFIX") {
            cfg.oracle.wallet.prefix = value;
        }
        if let Ok(value) = env::var("ORACLE_WALLET_DERIVATION_PATH") {
            cfg.oracle.wallet.derivation_path = value;
        }
        if let Ok(value) = env::var("ORACLE_FEE_DENOM") {
            cfg.oracle.wallet.fee_denom = value;
        }
        if let Ok(value) = env::var("ORACLE_FEE_AMOUNT") {
            cfg.oracle.wallet.fee_amount = value
                .parse()
                .map_err(|e| OracleError::Config(format!("invalid ORACLE_FEE_AMOUNT: {e}")))?;
        }
        if let Ok(value) = env::var("ORACLE_GAS_LIMIT") {
            cfg.oracle.wallet.gas_limit = value
                .parse()
                .map_err(|e| OracleError::Config(format!("invalid ORACLE_GAS_LIMIT: {e}")))?;
        }
        if let Ok(value) = env::var("ORACLE_MEMO") {
            cfg.oracle.wallet.memo = Some(value);
        }

        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), OracleError> {
        if self.oracle.probe_interval_secs == 0 || self.oracle.publish_interval_secs == 0 {
            return Err(OracleError::Config(
                "probe_interval_secs and publish_interval_secs must be > 0".to_string(),
            ));
        }
        if self.oracle.batch_size == 0 {
            return Err(OracleError::Config("batch_size must be > 0".to_string()));
        }
        if self.oracle.jitter_pct > 100 {
            return Err(OracleError::Config("jitter_pct must be <= 100".to_string()));
        }
        if self.oracle.max_concurrency == 0 {
            return Err(OracleError::Config("max_concurrency must be > 0".to_string()));
        }
        if self.oracle.lcd_endpoint.trim().is_empty() {
            return Err(OracleError::Config("lcd_endpoint must not be empty".to_string()));
        }
        if self.oracle.wallet.prefix.trim().is_empty() {
            return Err(OracleError::Config("wallet.prefix must not be empty".to_string()));
        }
        if self.oracle.wallet.derivation_path.trim().is_empty() {
            return Err(OracleError::Config(
                "wallet.derivation_path must not be empty".to_string(),
            ));
        }
        if self.oracle.wallet.fee_denom.trim().is_empty() {
            return Err(OracleError::Config("wallet.fee_denom must not be empty".to_string()));
        }
        Ok(())
    }
}
