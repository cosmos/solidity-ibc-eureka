use crate::error::{AggregatorError, Result};
use serde::Deserialize;
use std::{fs, net::SocketAddr, path::Path, str::FromStr};
use tracing::Level;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// The configuration for the aggregator server.
    pub server: ServerConfig,
    /// The configuration for the attestor signer.
    pub attestor: AttestorConfig,
}

impl Config {
    /// Load configuration from a TOML file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| AggregatorError::config_with_source(
                format!("Failed to read config file '{}'", path.display()),
                e
            ))?;

        let config: Config = toml::from_str(&content)
            .map_err(|e| AggregatorError::config_with_source(
                "Invalid TOML format in configuration file",
                e
            ))?;

        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        self.server.validate()?;
        self.attestor.validate()?;

        Ok(())
    }
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct AttestorConfig {
    pub attestor_query_timeout_ms: u64,
    pub quorum_threshold: usize,
    pub attestor_endpoints: Vec<String>,
}

impl AttestorConfig {
    pub fn validate(&self) -> std::result::Result<(), AggregatorError> {
        // Validate quorum threshold
        if self.quorum_threshold == 0 {
            return Err(AggregatorError::config(
                "quorum_threshold must be greater than 0"
            ));
        }

        if self.quorum_threshold > self.attestor_endpoints.len() {
            return Err(AggregatorError::config(format!(
                "quorum_threshold [{}] cannot exceed number of attestor endpoints [{}]",
                self.quorum_threshold,
                self.attestor_endpoints.len()
            )));
        }

        // Validate timeout
        if self.attestor_query_timeout_ms == 0 {
            return Err(AggregatorError::config(
                "attestor_query_timeout_ms must be greater than 0"
            ));
        }

        if self.attestor_query_timeout_ms > 60_000 {
            return Err(AggregatorError::config(
                "attestor_query_timeout_ms should not exceed 60 seconds"
            ));
        }

        // Validate endpoints
        if self.attestor_endpoints.is_empty() {
            return Err(AggregatorError::config(
                "at least one attestor endpoint must be specified"
            ));
        }

        for endpoint in &self.attestor_endpoints {
            if endpoint.trim().is_empty() {
                return Err(AggregatorError::config("attestor endpoint cannot be empty"));
            }

            // Basic URL validation - ensure it looks like a URL
            if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
                return Err(AggregatorError::config(format!(
                    "attestor endpoint '{endpoint}' must start with http:// or https://",
                )));
            }
        }

        Ok(())
    }
}

/// The configuration for the aggregator server.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct ServerConfig {
    /// The listener_addr to bind the server to.
    pub listener_addr: SocketAddr,
    /// The log level for the server.
    #[serde(default)]
    pub log_level: String,
}

impl ServerConfig {
    pub fn validate(&self) -> std::result::Result<(), AggregatorError> {
        if !self.log_level.is_empty() {
            Level::from_str(&self.log_level)
                .map_err(|_| AggregatorError::config(format!(
                    "invalid log level '{}'. Valid levels are: TRACE, DEBUG, INFO, WARN, ERROR",
                    self.log_level
                )))?;
        }

        Ok(())
    }

    /// Returns the log level for the server.
    #[must_use]
    pub fn log_level(&self) -> Level {
        Level::from_str(&self.log_level).unwrap_or(Level::INFO)
    }
}
