use crate::{aggregator::AggregatorService, config::Config};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use crate::rpc::{aggregator_server::AggregatorServer, FILE_DESCRIPTOR_SET};

/// Simple server that accepts inbound RPC calls for [AttestationServiceServer]
/// and periodically updates attestation state.
pub struct Server;

impl Server {
    pub fn new() -> Self {
        Self { }
    }

    /// Starts the [AggregatorService] RPC server with the provided configuration.
    pub async fn start(
        &self,
        service: AggregatorService,
        config: Config,
    ) -> Result<(), anyhow::Error> {
        tokio::spawn(async move {
            let socket_addr = config.listen_addr;
            let reflection_service = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
                .build_v1()
                .unwrap();

            tracing::info!("Started gRPC server on {}", socket_addr);
            tonic::transport::Server::builder()
                .layer(
                    TraceLayer::new_for_grpc()
                        .make_span_with(DefaultMakeSpan::new().include_headers(true))
                        .on_response(DefaultOnResponse::new().level(tracing::Level::INFO)),
                )
                .add_service(AggregatorServer::new(service))
                .add_service(reflection_service)
                .serve(socket_addr)
                .await
                .unwrap();
        });
        Ok(())
    }
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    use tonic::Request;
    use crate::{
        config::Config, 
        mock_attestor::setup_attestor_server,
        rpc::{aggregator_client::AggregatorClient, AggregateRequest}
    };
    use std::net::SocketAddr;
    use url::Url;

    #[tokio::test]
    async fn server_accepts_and_responds_to_rpc() {
        let (addr_1, pk_1) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0).await.unwrap();
        
        let listen_addr: SocketAddr = "127.0.0.1:50051".parse().unwrap();
        let config = Config {
            attestor_endpoints: vec![
                Url::parse(&format!("http://{addr_1}")).unwrap(),
                Url::parse(&format!("http://{addr_2}")).unwrap(),
            ],
            quorum_threshold: 2,
            listen_addr,
            attestor_query_timeout_ms: 500,
        };

        let service = AggregatorService::from_config(config.clone())
            .await
            .expect("failed to build AggregatorService");

        let server = Server::new();
        server.start(service, config.clone())
            .await
            .expect("server start failed");

        sleep(Duration::from_millis(100)).await;

        let endpoint = format!("http://{listen_addr}");
        let mut client = AggregatorClient::connect(endpoint)
            .await
            .expect("client connect failed");

        let req = Request::new(AggregateRequest { min_height: 11 });
        let resp = client.get_aggregate_attestation(req)
            .await
            .expect("RPC failed")
            .into_inner();

        assert_eq!(resp.height, 110, "default height should be 0");
        assert_eq!(resp.sig_pubkey_pairs.len(), 2);
        assert!(resp.sig_pubkey_pairs.iter().any(|pair| pair.pubkey == pk_1));
        assert!(resp.sig_pubkey_pairs.iter().any(|pair| pair.pubkey == pk_2));
        
    }
}
