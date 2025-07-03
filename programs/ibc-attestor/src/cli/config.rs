//! Defines the top level configuration for the attestor.
use std::{fs, path::Path, str::FromStr};

use thiserror::Error;
use tracing::Level;

/// The top level configuration for the relayer.
#[derive(Debug, serde::Deserialize)]
pub struct AttestorConfig {
    /// The configuration for the attestor server.
    pub server: ServerConfig,
    /// The configuration for the attestor signer.
    pub signer: SignerConfig,

    #[cfg(feature = "sol")]
    /// The configuration for the attestor signer.
    pub solana: SolanaClientConfig,
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
#[derive(Debug, serde::Deserialize)]
pub struct SolanaClientConfig {
    pub url: String,
    pub account_key: String,
}
