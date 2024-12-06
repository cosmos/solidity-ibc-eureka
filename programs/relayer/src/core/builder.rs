//! Defines the `RelayerBuilder` struct that is used to build the relayer server.

use std::{collections::HashMap, sync::Arc};

use futures::future;

use crate::cli::config::RelayerConfig;

use super::modules::ModuleServer;

/// The `RelayerBuilder` struct is used to build the relayer binary.
#[derive(Default)]
#[allow(clippy::module_name_repetitions)]
pub struct RelayerBuilder {
    /// The relayer modules to include in the relayer binary and their ports.
    modules: HashMap<String, Arc<dyn ModuleServer>>,
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
    pub fn add_module<T: ModuleServer>(&mut self, module: T) {
        assert!(
            !self.modules.contains_key(module.name()),
            "Relayer module already added"
        );
        self.modules
            .insert(module.name().to_string(), Arc::new(module));
    }

    /// Start the relayer server.
    #[allow(clippy::pedantic)]
    pub async fn start(self, config: RelayerConfig) -> anyhow::Result<()> {
        // Ensure the starting port and address are set
        let address = config.server.address;

        // Vector to store spawned tasks for each module
        let mut tasks = Vec::new();

        // Iterate through all registered modules
        for module_config in config.modules.into_iter() {
            if !module_config.enabled {
                continue;
            }

            let name = module_config.name;
            let config = module_config.config;
            let port = module_config.port;
            let module = self
                .modules
                .get(&name)
                .unwrap_or_else(|| {
                    panic!("Relayer module not found: {}", name);
                })
                .clone();

            // Construct the socket address
            let socket_addr = format!("{}:{}", address, port);
            tracing::info!(%name, %socket_addr, "Starting relayer module...");
            let socket_addr = socket_addr
                .parse::<std::net::SocketAddr>()
                .unwrap_or_else(|err| panic!("Failed to parse socket address: {}", err));

            tasks.push(tokio::spawn(async move {
                if let Err(err) = module.serve(config, socket_addr).await {
                    tracing::error!(%name, %err, "Failed to start module");
                }
            }));
        }

        // Wait for all tasks to complete
        future::try_join_all(tasks).await?;

        Ok(())
    }
}
