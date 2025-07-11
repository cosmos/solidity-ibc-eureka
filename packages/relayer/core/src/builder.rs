//! Defines the [`RelayerBuilder`] struct that is used to build the relayer server.

use std::collections::HashMap;

use crate::{
    api::{
        self,
        relayer_service_server::{RelayerService, RelayerServiceServer},
    },
    config::RelayerConfig,
};
use tonic::{transport::Server, Request, Response};

use super::modules::RelayerModule;

/// The `RelayerBuilder` struct is used to build the relayer.
#[derive(Default)]
#[allow(clippy::module_name_repetitions)]
pub struct RelayerBuilder {
    /// The relayer modules that can be used by the relayer to create services from configuration.
    modules: HashMap<String, Box<dyn RelayerModule>>,
}

/// The `Relayer` is a router that implements the [`RelayerService`] trait.
#[derive(Default)]
struct Relayer {
    /// Mapping of (`src_chain`, `dst_chain`) to the relayer service.
    services: HashMap<(String, String), Box<dyn RelayerService>>,
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
    pub fn add_module<T: RelayerModule>(&mut self, module: T) {
        assert!(
            !self.modules.contains_key(module.name()),
            "Relayer module already added"
        );
        self.modules
            .insert(module.name().to_string(), Box::new(module));
    }

    /// Start the relayer server.
    /// # Errors
    /// Returns an error if the server fails to start.
    pub async fn start(&self, config: RelayerConfig) -> anyhow::Result<()> {
        let socket_addr = format!("{}:{}", config.server.address, config.server.port);
        tracing::info!(%socket_addr, "Starting relayer...");
        let socket_addr = socket_addr.parse::<std::net::SocketAddr>()?;

        let reflection_service = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(api::FILE_DESCRIPTOR_SET)
            .build_v1()?; // Build the reflection service

        let mut relayer = Relayer::default();
        // Iterate through all configured modules
        for c in config.modules.into_iter().filter(|c| c.enabled) {
            let module =
                self.modules.get(&c.name).map(|v| &**v).ok_or_else(|| {
                    anyhow::anyhow!("Module {} not found in relayer builder", c.name)
                })?;
            relayer.add_module(
                c.src_chain,
                c.dst_chain,
                module.create_service(c.config).await?,
            );
        }

        // Start the gRPC server
        tracing::info!("Started gRPC server on {}", socket_addr);
        Server::builder()
            .add_service(RelayerServiceServer::new(relayer))
            .add_service(reflection_service)
            .serve(socket_addr)
            .await?;

        Ok(())
    }
}

impl Relayer {
    #[allow(clippy::result_large_err)]
    fn get_module(
        &self,
        src_chain: &str,
        dst_chain: &str,
    ) -> Result<&dyn RelayerService, tonic::Status> {
        self.services
            .get(&(src_chain.to_string(), dst_chain.to_string()))
            .map(|v| &**v)
            .ok_or_else(|| {
                tonic::Status::not_found(format!(
                    "Module not found for src_chain: {src_chain}, dst_chain: {dst_chain}",
                ))
            })
    }

    fn add_module(
        &mut self,
        src_chain: String,
        dst_chain: String,
        module: Box<dyn RelayerService>,
    ) {
        self.services.insert((src_chain, dst_chain), module);
    }
}

#[tonic::async_trait]
impl RelayerService for Relayer {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        let inner_request = request.get_ref();
        let src_chain = inner_request.src_chain.clone();
        let dst_chain = inner_request.dst_chain.clone();

        crate::metrics::track_metrics("info", &src_chain, &dst_chain, || async move {
            let inner_request = request.get_ref();
            self.get_module(&inner_request.src_chain, &inner_request.dst_chain)?
                .info(request)
                .await
                .map_err(|e| {
                    tracing::error!("Info request failed: {:?}", e);
                    tonic::Status::internal("Failed to get info. See logs for more details.")
                })
        })
        .await
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        let inner_request = request.get_ref();
        let src_chain = inner_request.src_chain.clone();
        let dst_chain = inner_request.dst_chain.clone();

        crate::metrics::track_metrics("relay_by_tx", &src_chain, &dst_chain, || async move {
            let inner_request = request.get_ref();
            self.get_module(&inner_request.src_chain, &inner_request.dst_chain)?
                .relay_by_tx(request)
                .await
                .map_err(|e| {
                    tracing::error!("Relay by tx request failed: {:?}", e);
                    tonic::Status::internal("Failed to relay by tx. See logs for more details.")
                })
        })
        .await
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        let inner_request = request.get_ref();
        let src_chain = inner_request.src_chain.clone();
        let dst_chain = inner_request.dst_chain.clone();

        crate::metrics::track_metrics("create_client", &src_chain, &dst_chain, || async move {
            let inner_request = request.get_ref();
            self.get_module(&inner_request.src_chain, &inner_request.dst_chain)?
                .create_client(request)
                .await
                .map_err(|e| {
                    tracing::error!("Create client request failed: {:?}", e);
                    tonic::Status::internal("Failed to create client. See logs for more details.")
                })
        })
        .await
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        let inner_request = request.get_ref();
        let src_chain = inner_request.src_chain.clone();
        let dst_chain = inner_request.dst_chain.clone();

        crate::metrics::track_metrics("update_client", &src_chain, &dst_chain, || async move {
            let inner_request = request.get_ref();
            self.get_module(&inner_request.src_chain, &inner_request.dst_chain)?
                .update_client(request)
                .await
                .map_err(|e| {
                    tracing::error!("Update client request failed: {:?}", e);
                    tonic::Status::internal("Failed to update client. See logs for more details.")
                })
        })
        .await
    }
}
