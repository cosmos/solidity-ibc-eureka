//! Defines the `RelayerBuilder` struct that is used to build the relayer server.

use std::{collections::HashMap, sync::Arc};

use futures::future;

use crate::{
    api::{self, relayer_service_server::RelayerService},
    cli::config::RelayerConfig,
};
use tonic::{Request, Response};

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
}

#[tonic::async_trait]
impl RelayerService for RelayerBuilder {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        todo!()
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        todo!()
    }
}
