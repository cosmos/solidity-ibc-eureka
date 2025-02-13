//! Defines the `RelayerBuilder` struct that is used to build the relayer server.

use std::{collections::HashMap, sync::Arc};

use futures::future;

use crate::cli::config::RelayerConfig;

use super::modules::ModuleServer;

/// The `RelayerBuilder` struct is used to build the relayer binary.
#[derive(Default)]
#[allow(clippy::module_name_repetitions)]
pub struct Relayer {
    /// The relayer modules to include in the relayer binary and their ports.
    modules: HashMap<String, Arc<dyn ModuleServer>>,
}

impl Relayer {
    /// Create a new `RelayerBuilder` instance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a relayer module to the relayer binary.
    /// # Panics
    /// Panics if the module has already been added.
    pub fn add_module<T: ModuleServer>(&mut self, module: T) {
        assert!(
            !self.modules.contains_key(module.name()),
            "Relayer module already added"
        );
        self.modules
            .insert(module.name().to_string(), Arc::new(module));
    }
}
