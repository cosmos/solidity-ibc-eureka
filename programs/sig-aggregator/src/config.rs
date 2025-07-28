use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashSet, fs, net::SocketAddr, path::Path, str::FromStr};
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
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file '{}'", path.display()))?;

        let config: Config = toml::from_str(&content)
            .context("Failed to parse TOML configuration")?;

        config.validate()?;

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
    fn validate(&self) -> Result<()> {
        anyhow::ensure!(
            self.quorum_threshold >= defaults::MIN_QUORUM_THRESHOLD,
            "quorum_threshold must be >= {}",
            defaults::MIN_QUORUM_THRESHOLD
        );

        anyhow::ensure!(
            self.quorum_threshold <= self.attestor_endpoints.len(),
            "quorum_threshold ({}) cannot exceed number of attestor endpoints ({})",
            self.quorum_threshold,
            self.attestor_endpoints.len()
        );

        anyhow::ensure!(
            self.attestor_query_timeout_ms >= defaults::MIN_TIMEOUT_MS,
            "attestor_query_timeout_ms must be >= {}ms",
            defaults::MIN_TIMEOUT_MS
        );

        anyhow::ensure!(
            self.attestor_query_timeout_ms <= defaults::MAX_TIMEOUT_MS,
            "attestor_query_timeout_ms must be <= {}ms",
            defaults::MAX_TIMEOUT_MS
        );

        anyhow::ensure!(
            !self.attestor_endpoints.is_empty(),
            "at least one attestor endpoint must be specified"
        );

        for (i, endpoint) in self.attestor_endpoints.iter().enumerate() {
            anyhow::ensure!(
                !endpoint.trim().is_empty(),
                "endpoint at index {i} is empty"
            );

            anyhow::ensure!(
                endpoint.starts_with("http://") || endpoint.starts_with("https://"),
                "endpoint '{endpoint}' must start with http:// or https://"
            );
        }

        // Check for duplicate endpoints
        let mut unique_endpoints = HashSet::new();
        for endpoint in &self.attestor_endpoints {
            anyhow::ensure!(
                unique_endpoints.insert(endpoint),
                "duplicate endpoint: '{endpoint}'"
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
    #[serde(default = "defaults::default_log_level")]
    pub log_level: String,
}

impl ServerConfig {
    fn validate(&self) -> Result<()> {
        if !self.log_level.is_empty() {
            Level::from_str(&self.log_level)
                .with_context(|| format!(
                    "invalid log level '{}'. Valid levels are: TRACE, DEBUG, INFO, WARN, ERROR",
                    self.log_level
                ))?;
        }
        Ok(())
    }

    /// Returns the log level for the server.
    #[must_use]
    pub fn log_level(&self) -> Level {
        Level::from_str(&self.log_level).unwrap_or(defaults::DEFAULT_LOG_LEVEL)
    }
}

/// Default values for configuration
mod defaults {
    use tracing::Level;

    pub const DEFAULT_LOG_LEVEL: Level = Level::INFO;
    pub const MIN_TIMEOUT_MS: u64 = 10;
    pub const MAX_TIMEOUT_MS: u64 = 60_000;
    pub const MIN_QUORUM_THRESHOLD: usize = 1;

    pub fn default_log_level() -> String {
        DEFAULT_LOG_LEVEL.to_string().to_lowercase()
    }
}
