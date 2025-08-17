//! Defines the top level configuration for the attestor.
use std::{
    cell::LazyCell,
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use thiserror::Error;
use tracing::Level;

#[cfg(feature = "arbitrum")]
use crate::ArbitrumClientConfig;
#[cfg(feature = "op")]
use crate::OpClientConfig;
#[cfg(feature = "eth")]
use crate::EthClientConfig;
#[cfg(feature = "cosmos")]
use crate::CosmosClientConfig;

pub const IBC_ATTESTOR_DIR: LazyCell<PathBuf> = LazyCell::new(|| {
    env::home_dir()
        .map(|home| home.join(".ibc-attestor"))
        .unwrap()
});

const IBC_ATTESTOR_FILE: &str = "ibc-attestor.pem";

pub const IBC_ATTESTOR_PATH: LazyCell<PathBuf> = LazyCell::new(|| {
    env::home_dir()
        .map(|home| home.join(&*IBC_ATTESTOR_DIR).join(IBC_ATTESTOR_FILE))
        .unwrap()
});

/// The top level configuration for the relayer.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct AttestorConfig {
    /// The configuration for the attestor server.
    pub server: ServerConfig,
    /// The configuration for the attestor signer.
    pub signer: Option<SignerConfig>,

    #[cfg(feature = "sol")]
    /// The configuration for the Solana client.
    pub solana: SolanaClientConfig,

    #[cfg(feature = "op")]
    /// The configuration for the Optimism client.
    pub op: OpClientConfig,

    #[cfg(feature = "arbitrum")]
    /// The configuration for the Arbitrum client.
    pub arbitrum: ArbitrumClientConfig,

    #[cfg(feature = "eth")]
    /// The configuration for the Ethereum client.
    pub ethereum: EthClientConfig,

    #[cfg(feature = "cosmos")]
    /// The configuration for the Cosmos client.
    pub cosmos: CosmosClientConfig,
}

impl AttestorConfig {
    /// Load an `AttestorConfig` from a TOML file on disk.
    ///
    /// Accepts any `P: AsRef<Path>` (e.g. &str, String, Path, PathBuf).
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let path_ref = path.as_ref();
        let contents = fs::read_to_string(path_ref)
            .map_err(|e| ConfigError::Io(path_ref.display().to_string(), e))?;
        let cfg = toml::from_str(&contents)?;
        Ok(cfg)
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct SignerConfig {
    pub secret_key: String,
}

impl Default for SignerConfig {
    fn default() -> Self {
        SignerConfig {
            // Unwrap safe as path defined with valid utf-8
            secret_key: IBC_ATTESTOR_PATH.to_str().unwrap().into(),
        }
    }
}

/// The configuration for the relayer server.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ServerConfig {
    /// The address to bind the server to.
    pub address: String,
    /// The port to bind the server to.
    pub port: u16,
    /// The log level for the server.
    #[serde(default)]
    pub log_level: String,
}

/// Errors that can occur loading the attestor config.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("I/O error reading `{0}`: {1}")]
    Io(String, #[source] std::io::Error),

    #[error("invalid TOML in config: {0}")]
    Toml(#[from] toml::de::Error),
}

impl ServerConfig {
    /// Returns the log level for the server.
    #[must_use]
    pub fn log_level(&self) -> Level {
        Level::from_str(&self.log_level).unwrap_or(Level::INFO)
    }
}

#[cfg(feature = "sol")]
#[derive(Clone, Debug, serde::Deserialize)]
pub struct SolanaClientConfig {
    pub url: String,
    pub account_key: String,
}

// CosmosClientConfig is defined in `adapter_impls::cosmos::config` and re-exported at crate root.
