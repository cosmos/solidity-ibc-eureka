//! Defines the `RelayerBuilder` struct that is used to build the relayer server.

use super::modules::RelayerModuleServer;

/// The `RelayerBuilder` struct is used to build the relayer binary.
#[derive(Default)]
#[allow(clippy::module_name_repetitions)]
pub struct RelayerBuilder {
    /// The relayer modules to include in the relayer binary.
    modules: Vec<Box<dyn RelayerModuleServer>>,
}

impl RelayerBuilder {
    /// Create a new `RelayerBuilder` instance.
    #[must_use]
    pub const fn new() -> Self {
        Self { modules: vec![] }
    }

    /// Add a relayer module to the relayer binary.
    pub fn add_module(&mut self, module: Box<dyn RelayerModuleServer>) {
        self.modules.push(module);
    }
}
