use std::{marker::PhantomData, sync::Arc};

use tonic::transport::Server as TonicServer;

use crate::{
    adapter_client::Adapter,
    api::{self, attestation_service_server::AttestationServiceServer},
    attestor::{Attestor, AttestorService},
    cli::ServerConfig,
};

pub struct Server<A> {
    _data: PhantomData<A>,
}

impl<A> Server<A>
where
    A: Adapter,
{
    pub async fn start(
        &self,
        service: AttestorService<A>,
        server_config: ServerConfig,
    ) -> Result<(), anyhow::Error> {
        let service = Arc::new(service);

        let server_service = service.clone();
        tokio::spawn(async move {
            let socket_addr = format!("{}:{}", server_config.address, server_config.port);
            tracing::info!(%socket_addr, "Starting relayer...");
            let socket_addr = socket_addr.parse::<std::net::SocketAddr>().unwrap();
            let reflection_service = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(api::FILE_DESCRIPTOR_SET)
                .build_v1()
                .unwrap(); // Build the reflection service
            tracing::info!("Started gRPC server on {}", socket_addr);
            TonicServer::builder()
                .add_service(AttestationServiceServer::new(server_service.clone()))
                .add_service(reflection_service)
                .serve(socket_addr)
                .await
                .unwrap();
        });

        let mut attestor_ticker = tokio::time::interval(service.update_frequency());
        loop {
            tokio::select! {
                _ = attestor_ticker.tick() => {
                    tracing::debug!("Updating attestor heights");
                    service.update_attestation_store().await;
                }
            }
        }
    }

    pub fn new() -> Self {
        Self { _data: PhantomData }
    }
}
