use anyhow::{Context, Result};
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
            .context(format!("Failed to read config file: {}", path.display()))?;

        let config: Config = toml::from_str(&content)
            .context("Failed to parse TOML configuration")?;

        config.validate()
            .context("Configuration validation failed")?;

        Ok(config)
    }

    #[allow(clippy::result_large_err)]
    pub fn validate(&self) -> anyhow::Result<()> {
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
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.quorum_threshold > 0,
            "quorum_threshold must be greater than 0"
        );

        anyhow::ensure!(
            self.quorum_threshold <= self.attestor_endpoints.len(),
            "quorum_threshold ({}) cannot exceed number of attestor endpoints ({})",
            self.quorum_threshold,
            self.attestor_endpoints.len()
        );

        anyhow::ensure!(
            self.attestor_query_timeout_ms > 0,
            "attestor_query_timeout_ms must be greater than 0"
        );

        anyhow::ensure!(
            self.attestor_query_timeout_ms <= 60_000,
            "attestor_query_timeout_ms should not exceed 60 seconds"
        );

        anyhow::ensure!(
            !self.attestor_endpoints.is_empty(),
            "at least one attestor endpoint must be specified"
        );

        for endpoint in &self.attestor_endpoints {
            anyhow::ensure!(
                !endpoint.trim().is_empty(),
                "attestor endpoint cannot be empty"
            );

            anyhow::ensure!(
                endpoint.starts_with("http://") || endpoint.starts_with("https://"),
                "attestor endpoint '{}' must start with http:// or https://",
                endpoint
            );
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
    fn validate(&self) -> Result<()> {
        if !self.log_level.is_empty() {
            Level::from_str(&self.log_level).map_err(|_| {
                anyhow::anyhow!(
                    "invalid log level '{}'. Valid levels are: TRACE, DEBUG, INFO, WARN, ERROR",
                    self.log_level
                )
            })?;
        }
        Ok(())
    }

    /// Returns the log level for the server.
    #[must_use]
    pub fn log_level(&self) -> Level {
        Level::from_str(&self.log_level).unwrap_or(Level::INFO)
    }
}
