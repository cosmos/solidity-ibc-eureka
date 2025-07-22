use crate::{
    aggregator::AggregatorService,
    config::ServerConfig,
    error::{AggregatorError, Result},
    rpc::{aggregator_server::AggregatorServer, AGG_FILE_DESCRIPTOR},
};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};

/// Starts the [AggregatorService] RPC server with the provided configuration.
pub async fn start(service: AggregatorService, config: ServerConfig) -> Result<(), anyhow::Error> {
    tracing::info!("Starting Server With Config: {:?}", config);
    let socket_addr = config.listener_addr;
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(AGG_FILE_DESCRIPTOR)
        .build_v1()
        .map_err(|e| AggregatorError::internal_with_source(
            "Failed to build reflection service",
            e
        ))?;

    tonic::transport::Server::builder()
        .layer(
            TraceLayer::new_for_grpc()
                .make_span_with(DefaultMakeSpan::new().include_headers(true))
                .on_response(DefaultOnResponse::new().level(config.log_level())),
        )
        .add_service(AggregatorServer::new(service))
        .add_service(reflection_service)
        .serve(socket_addr)
        .await
        .map_err(|e| AggregatorError::internal_with_source(
            format!("Failed to start server on {socket_addr}"),
            e
        ))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{AttestorConfig, Config, ServerConfig},
        mock_attestor::setup_attestor_server,
        rpc::{aggregator_client::AggregatorClient, AggregateRequest},
    };
    use tokio::time::{sleep, Duration};
    use tonic::Request;

    #[tokio::test]
    async fn server_accepts_and_responds_to_rpc() {
        let (addr_1, pk_1) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0).await.unwrap();

        let listener_addr: String = "127.0.0.1:50051".to_string();
        let config = Config {
            server: ServerConfig {
                listener_addr: listener_addr.parse().unwrap(),
                log_level: "INFO".to_string(),
            },
            attestor: AttestorConfig {
                attestor_query_timeout_ms: 500,
                quorum_threshold: 2,
                attestor_endpoints: vec![format!("http://{addr_1}"), format!("http://{addr_2}")],
            },
        };

        let service = AggregatorService::from_config(config.attestor)
            .await
            .expect("failed to build AggregatorService");

        let server_handle = tokio::spawn({
            async move {
                start(service, config.server)
                    .await
                    .expect("server start failed");
            }
        });

        sleep(Duration::from_millis(100)).await;

        let endpoint = format!("http://{listener_addr}");
        let mut client = AggregatorClient::connect(endpoint)
            .await
            .expect("client connect failed");

        let req = Request::new(AggregateRequest { min_height: 11 });
        let resp = client
            .get_aggregate_attestation(req)
            .await
            .expect("RPC failed")
            .into_inner();

        assert_eq!(resp.height, 110);
        assert_eq!(resp.sig_pubkey_pairs.len(), 2);
        assert!(resp.sig_pubkey_pairs.iter().any(|pair| pair.pubkey == pk_1));
        assert!(resp.sig_pubkey_pairs.iter().any(|pair| pair.pubkey == pk_2));
        server_handle.abort();
    }
}
