use anyhow::Result;
use serde::Deserialize;
use std::{fs, net::SocketAddr, path::Path, str::FromStr};
use thiserror::Error;
use tracing::Level;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// The configuration for the aggregator server.
    pub server: ServerConfig,
    /// The configuration for the attestor signer.
    pub attestor: AttestorConfig,
}

impl Config {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct AttestorConfig {
    pub attestor_query_timeout_ms: u64,
    pub quorum_threshold: usize,
    pub attestor_endpoints: Vec<String>,
}

/// The configuration for the relayer server.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ServerConfig {
    /// The listner_addr to bind the server to.
    pub listner_addr: SocketAddr,
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
