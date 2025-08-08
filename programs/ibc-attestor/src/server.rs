use std::{marker::PhantomData, sync::Arc};

use tonic::transport::Server as TonicServer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};

use crate::{
    adapter_client::AttestationAdapter,
    api::{self, attestation_service_server::AttestationServiceServer},
    attestor::AttestorService,
    cli::{AttestorConfig, ServerConfig},
    signer::Signer,
    AttestorError,
};

#[cfg(feature = "arbitrum")]
use crate::ArbitrumClient;
#[cfg(feature = "op")]
use crate::OpClient;
#[cfg(feature = "sol")]
use crate::SolanaClient;

/// Simple server that accepts inbound RPC calls for [AttestationServiceServer]
/// and periodically updates attestation state.
pub struct Server<A> {
    _data: PhantomData<A>,
}

impl<A> Server<A>
where
    A: AttestationAdapter,
{
    /// Starts the [AttestorService] RPC server and attestation store
    /// updates.
    pub async fn start(
        &self,
        service: AttestorService<A>,
        server_config: ServerConfig,
    ) -> Result<(), anyhow::Error> {
        let service = Arc::new(service);

        let server_service = service.clone();
        run_rpc_inbound_server(server_service, server_config).await?;
        Ok(())
    }

    pub fn new(server_config: &ServerConfig) -> Self {
        tracing_subscriber::fmt::fmt()
            .with_max_level(server_config.log_level())
            .init();
        Self { _data: PhantomData }
    }
}

async fn run_rpc_inbound_server<A>(
    server_service: Arc<AttestorService<A>>,
    server_config: ServerConfig,
) -> Result<(), AttestorError>
where
    A: AttestationAdapter,
{
    let socket_addr = format!("{}:{}", server_config.address, server_config.port);
    tracing::info!(%socket_addr, "Starting relayer...");
    let socket_addr = socket_addr
        .parse::<std::net::SocketAddr>()
        .map_err(|e| AttestorError::ServerConfigError(e.to_string()))?;

    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(api::FILE_DESCRIPTOR_SET)
        .build_v1()
        .map_err(|e| AttestorError::ServerConfigError(e.to_string()))?;

    tracing::info!("Started gRPC server on {}", socket_addr);
    TonicServer::builder()
        .layer(
            TraceLayer::new_for_grpc()
                // include request headers in the span metadata…
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                // …and log when the response is sent
                .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
        )
        .add_service(AttestationServiceServer::new(server_service.clone()))
        .add_service(reflection_service)
        .serve(socket_addr)
        .await
        .map_err(|e| AttestorError::ServerConfigError(e.to_string()))?;
    Ok(())
}

#[cfg(feature = "sol")]
pub async fn run_solana_server(config: AttestorConfig) -> Result<(), anyhow::Error> {
    let signer = Signer::from_config(config.signer.unwrap_or_default())?;
    let adapter = SolanaClient::from_config(config.solana);
    let attestor = AttestorService::new(adapter, signer);
    let server = Server::new(&config.server);
    server.start(attestor, config.server).await
}

#[cfg(feature = "op")]
pub async fn run_optimism_server(config: AttestorConfig) -> Result<(), anyhow::Error> {
    let signer = Signer::from_config(config.signer.unwrap_or_default())?;
    let adapter = OpClient::from_config(&config.op)?;
    let attestor = AttestorService::new(adapter, signer);
    let server = Server::new(&config.server);
    server.start(attestor, config.server).await
}

#[cfg(feature = "arbitrum")]
pub async fn run_arbitrum_server(config: AttestorConfig) -> Result<(), anyhow::Error> {
    let signer = Signer::from_config(config.signer.unwrap_or_default())?;
    let adapter = ArbitrumClient::from_config(&config.arbitrum)?;
    let attestor = AttestorService::new(adapter, signer);
    let server = Server::new(&config.server);
    server.start(attestor, config.server).await
}

/// Blockchain-specific server startup functions

#[cfg(feature = "sol")]
pub async fn run_solana_server(config: AttestorConfig) -> Result<(), anyhow::Error> {
    let signer = Signer::from_config(config.signer.unwrap_or_default())?;
    let adapter = SolanaClient::from_config(config.solana);
    let attestor = AttestorService::new(adapter, signer);
    let server = Server::new(&config.server);
    server.start(attestor, config.server).await
}

#[cfg(feature = "op")]
pub async fn run_optimism_server(config: AttestorConfig) -> Result<(), anyhow::Error> {
    let signer = Signer::from_config(config.signer.unwrap_or_default())?;
    let adapter = OpClient::from_config(&config.op);
    let attestor = AttestorService::new(adapter, signer);
    let server = Server::new(&config.server);
    server.start(attestor, config.server).await
}

#[cfg(feature = "arbitrum")]
pub async fn run_arbitrum_server(config: AttestorConfig) -> Result<(), anyhow::Error> {
    let signer = Signer::from_config(config.signer.unwrap_or_default())?;
    let adapter = ArbitrumClient::from_config(&config.arbitrum);
    let attestor = AttestorService::new(adapter, signer);
    let server = Server::new(&config.server);
    server.start(attestor, config.server).await
}
