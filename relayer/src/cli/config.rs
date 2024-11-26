//! Defines the top level configuration for the relayer.

use serde_json::Value;

/// The top level configuration for the relayer.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::module_name_repetitions)]
pub struct RelayerConfig {
    /// The configuration for the relayer modules.
    pub modules: Vec<ModuleConfig>,
}

/// The configuration for the relayer modules.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::module_name_repetitions)]
pub struct ModuleConfig {
    /// The name of the module.
    pub name: String,
    /// The custom configuration for the module.
    pub config: Value,
    /// Whether the module is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// Returns true, used as a default value for boolean fields.
const fn default_true() -> bool {
    true
}
