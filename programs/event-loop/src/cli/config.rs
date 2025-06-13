//! Defines the top level configuration for the relayer.

use std::str::FromStr;

use tracing::Level;

/// The top level configuration for the relayer.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AttestorConfig {
    /// The configuration for the relayer server.
    pub server: ServerConfig,
}

/// The configuration for the relayer server.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct ServerConfig {
    /// The address to bind the server to.
    pub address: String,
    /// The port to bind the server to.
    pub port: u16,
    /// The log level for the server.
    #[serde(default)]
    pub log_level: String,
}

impl ServerConfig {
    /// Returns the log level for the server.
    #[must_use]
    pub fn log_level(&self) -> Level {
        Level::from_str(&self.log_level).unwrap_or(Level::INFO)
    }
}
