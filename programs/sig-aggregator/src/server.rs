use crate::{
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{AttestorConfig, Config, ServerConfig},
        mock_attestor::setup_attestor_server,
        rpc::{aggregator_service_client::AggregatorServiceClient, GetStateAttestationRequest},
    };
    use tokio::time::{sleep, Duration};
    use tonic::{Code as StatusCode, Request};

    #[tokio::test]
    async fn server_accepts_and_responds_to_rpc() {
        let (addr_1, pk_1) = setup_attestor_server(false, 0, 1).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0, 2).await.unwrap();

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
            cache: Default::default(),
        };

        let server_config = config.server.clone();
        let service = Aggregator::from_config(config)
            .await
            .expect("failed to build Aggregator Service");

        let server_handle = tokio::spawn({
            async move {
                start(service, server_config)
                    .await
                    .expect("server start failed");
            }
        });

        sleep(Duration::from_millis(100)).await;

        let endpoint = format!("http://{listener_addr}");
        let mut client = AggregatorServiceClient::connect(endpoint)
            .await
            .expect("client connect failed");

        // Check validation fails on empty packets.
        let req = Request::new(GetStateAttestationRequest {
            packets: vec![],
            height: 110,
        });
        let resp = client.get_state_attestation(req).await;
        assert!(resp.is_err());
        assert_eq!(resp.err().unwrap().code(), StatusCode::InvalidArgument);

        let req = Request::new(GetStateAttestationRequest {
            packets: vec![vec![1, 2, 3]],
            height: 110,
        });

        let resp = client
            .get_state_attestation(req)
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
