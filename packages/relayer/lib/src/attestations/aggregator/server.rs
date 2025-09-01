use super::{
    aggregator::Aggregator,
    config::ServerConfig,
    rpc::{aggregator_service_server::AggregatorServiceServer, AGG_FILE_DESCRIPTOR},
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};

/// Starts the [AggregatorService] RPC server with the provided configuration.
pub async fn start(service: Aggregator, config: ServerConfig) -> anyhow::Result<()> {
    tracing::info!("Starting aggregator server on {}", config.listener_addr);

    let socket_addr = config.listener_addr;
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(AGG_FILE_DESCRIPTOR)
        .build_v1()?;

    tonic::transport::Server::builder()
        .layer(
            TraceLayer::new_for_grpc()
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_response(DefaultOnResponse::new().level(config.log_level())),
        )
        .add_service(AggregatorServiceServer::new(service))
        .add_service(reflection_service)
        .serve(socket_addr)
        .await?;

    Ok(())
}
