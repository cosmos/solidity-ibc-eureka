//! Defines the top level configuration for the relayer.

use std::str::FromStr;

use serde_json::Value;
use tracing::Level;

/// The top level configuration for the relayer.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::module_name_repetitions)]
pub struct RelayerConfig {
    /// The configuration for the relayer modules.
    pub modules: Vec<ModuleConfig>,
    /// The configuration for the relayer server.
    pub server: ServerConfig,
}

/// The configuration for the relayer modules.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::module_name_repetitions)]
pub struct ModuleConfig {
    /// The name of the module.
    pub name: String,
    /// The source chain identifier for the module.
    /// Used to route requests to the correct module.
    pub src_chain: String,
    /// The destination chain identifier for the module.
    /// Used to route requests to the correct module.
    pub dst_chain: String,
    /// The custom configuration for the module.
    pub config: Value,
    /// Whether the module is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// The configuration for the relayer server.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::module_name_repetitions)]
pub struct ServerConfig {
    /// The address to bind the server to.
    pub address: String,
    /// The port to bind the server to.
    pub port: u16,
    /// The log level for the server.
    #[serde(default)]
    pub log_level: String,
}

/// Returns true, used as a default value for boolean fields.
const fn default_true() -> bool {
    true
}

impl ServerConfig {
    /// Returns the log level for the server.
    #[must_use]
    pub fn log_level(&self) -> Level {
        Level::from_str(&self.log_level).unwrap_or(Level::INFO)
    }
}

/// Parse a module configuration value into the target struct while producing
/// detailed path-aware error messages.
///
/// This helper is placed in `relayer_core` so that every relayer module can
/// reuse it without code duplication. It leverages `serde_path_to_error` to
/// include the exact JSON path of the failure (e.g. `sp1_programs.update_client`).
///
/// # Errors
/// Returns an [`anyhow::Error`] with the precise path and the original serde
/// error message.
pub fn parse_config<T>(value: serde_json::Value) -> anyhow::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    // Convert the value back to a string so that the JSON deserializer can
    // provide line/column information. This conversion is cheap compared to
    // the benefit of detailed error messages and we only do it once at startup.
    let json_string = value.to_string();

    let mut deserializer = serde_json::Deserializer::from_str(&json_string);
    match serde_path_to_error::deserialize::<_, T>(&mut deserializer) {
        Ok(v) => Ok(v),
        Err(e) => Err(anyhow::anyhow!(format!(
            "config error at {}: {}",
            e.path(),
            e
        ))),
    }
}
