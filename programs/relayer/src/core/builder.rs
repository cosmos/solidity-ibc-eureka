//! Defines the `RelayerBuilder` struct that is used to build the relayer server.

use std::collections::HashMap;

use futures::future;

use super::modules::RelayerModuleServer;

/// The `RelayerBuilder` struct is used to build the relayer binary.
#[derive(Default)]
#[allow(clippy::module_name_repetitions)]
pub struct RelayerBuilder {
    /// The relayer modules to include in the relayer binary.
    modules: HashMap<String, Box<dyn RelayerModuleServer>>,
    /// The starting port for the relayer binary.
    starting_port: Option<u16>,
    /// The address to bind the relayer server to.
    address: Option<String>,
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

    /// Set the starting port for the relayer binary.
    /// # Panics
    /// Panics if the starting port has already been set.
    pub fn set_starting_port(&mut self, starting_port: u16) {
        assert!(self.starting_port.is_none(), "Starting port already set");
        self.starting_port = Some(starting_port);
    }

    /// Set the address to bind the relayer server to.
    /// # Panics
    /// Panics if the address has already been set.
    pub fn set_address(&mut self, address: &str) {
        assert!(self.address.is_none(), "Address already set");
        self.address = Some(address.to_string());
    }

    /// Start the relayer server.
    #[allow(clippy::pedantic)]
    pub async fn start_server(self) -> anyhow::Result<()> {
        // Ensure the starting port and address are set
        let starting_port = self
            .starting_port
            .expect("Starting port must be set before starting the server");
        let address = self
            .address
            .as_ref()
            .expect("Address must be set before starting the server");

        // Vector to store spawned tasks for each module
        let mut tasks = Vec::new();

        // Iterate through all registered modules
        for (index, (name, module)) in self.modules.into_iter().enumerate() {
            // Calculate the port for this module, panic if overflow
            let port = starting_port
                .checked_add(index as u16)
                .expect("Port overflow");

            // Construct the socket address
            let socket_addr = format!("{}:{}", address, port);

            // Log the module and address
            tracing::info!(%name, %socket_addr, "Starting relayer module...");

            // Clone the module and socket address for the async task
            let socket_addr = socket_addr.parse::<std::net::SocketAddr>()?;

            // Spawn an async task to run the module's server
            tasks.push(tokio::spawn(async move {
                if let Err(err) = module.serve(socket_addr).await {
                    tracing::error!(%name, %err, "Failed to start module");
                }
            }));
        }

        // Wait for all tasks to complete
        future::try_join_all(tasks).await?;

        Ok(())
    }
}
