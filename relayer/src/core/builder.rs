//! Defines the `RelayerBuilder` struct that is used to build the relayer server.

use std::collections::HashMap;

use super::modules::RelayerModuleServer;

/// The `RelayerBuilder` struct is used to build the relayer binary.
#[derive(Default)]
#[allow(clippy::module_name_repetitions)]
pub struct RelayerBuilder {
    /// The relayer modules to include in the relayer binary.
    modules: HashMap<String, Box<dyn RelayerModuleServer>>,
}

impl RelayerBuilder {
    /// Create a new `RelayerBuilder` instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a relayer module to the relayer binary.
    /// # Panics
    /// Panics if the module has already been added.
    pub fn add_module(&mut self, name: &str, module: Box<dyn RelayerModuleServer>) {
        assert!(
            !self.modules.contains_key(name),
            "Relayer module already added"
        );
        self.modules.insert(name.to_string(), module);
    }
}
