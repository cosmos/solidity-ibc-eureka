use crate::{
    attestor_data::AttestatorData,
    config::{AttestorConfig, Config},
    rpc::{
        aggregator_service_server::AggregatorService,
        attestation_service_client::AttestationServiceClient, AggregatedAttestation, Attestation,
        GetAttestationsRequest, GetAttestationsResponse, PacketAttestationRequest,
        StateAttestationRequest,
    },
};
use futures::future::join_all;
use moka::future::Cache;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::{
    sync::Mutex,
    time::{timeout, Duration},
};
use tonic::{transport::Channel, Request, Response, Status};
use tracing::{error as tracing_error, instrument};

#[derive(Clone)]
enum AttestationQuery {
    Packet(Vec<Vec<u8>>, u64),
    State(u64),
}

type AttestorClient = (Arc<String>, Mutex<AttestationServiceClient<Channel>>);

/// Signature aggregator service that collects and aggregates attestations from multiple attestors.
/// # Architecture
/// The aggregator operates in two phases:
/// 1. **Packet Attestation**: Queries attestors for attestations of specific packets at a height
/// 2. **State Attestation**: Queries attestors for state attestations at the determined height
///
/// Both phases require a quorum of valid attestations before proceeding. Results are cached
/// using the following cache keys:
/// - State cache: `height -> aggregated attestation`
/// - Packet cache: `(packets_hash, height) -> aggregated attestation`
#[derive(Debug)]
pub struct Aggregator {
    quorum_threshold: usize,
    attestor_timeout_duration: Duration,
    attestor_clients: Vec<AttestorClient>,
    state_cache: Cache<u64, AggregatedAttestation>,
    packet_cache: Cache<([u8; 32], u64), AggregatedAttestation>,
}

impl Aggregator {
    pub async fn from_config(config: Config) -> anyhow::Result<Self> {
        let attestor_clients = Self::create_clients(&config.attestor).await?;

        Ok(Self {
            quorum_threshold: config.attestor.quorum_threshold,
            attestor_timeout_duration: Duration::from_millis(
                config.attestor.attestor_query_timeout_ms,
            ),
            attestor_clients,
            state_cache: Cache::new(config.cache.state_cache_max_entries),
            packet_cache: Cache::new(config.cache.packet_cache_max_entries),
        })
    }

    async fn create_clients(config: &AttestorConfig) -> anyhow::Result<Vec<AttestorClient>> {
        let futures = config.attestor_endpoints.iter().map(|endpoint| async move {
            AttestationServiceClient::connect(endpoint.clone())
                .await
                .map(|client| (Arc::new(endpoint.clone()), Mutex::new(client)))
        });

        join_all(futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow::anyhow!("Failed to connect to attestor: {e}"))
    }
}

#[tonic::async_trait]
impl AggregatorService for Aggregator {
    #[instrument(skip_all, fields(height = request.get_ref().height))]
    async fn get_attestations(
        &self,
        request: Request<GetAttestationsRequest>,
    ) -> Result<Response<GetAttestationsResponse>, Status> {
        let request = request.into_inner();

        if request.packets.is_empty() {
            return Err(Status::invalid_argument("Packets cannot be empty"));
        }

        if request.packets.iter().any(|packet| packet.is_empty()) {
            return Err(Status::invalid_argument("Packet cannot be empty"));
        }

        let mut sorted_packets = request.packets;
        sorted_packets.sort();
        let packet_cache_key = Self::make_packet_cache_key(&sorted_packets, request.height);

        let packet_attestation = self
            .packet_cache
            .try_get_with(packet_cache_key, async {
                let packet_attestations = self
                    .query_attestations(AttestationQuery::Packet(sorted_packets, request.height))
                    .await?;

                let quorumed_aggregation =
                    agg_quorumed_attestations(self.quorum_threshold, packet_attestations)
                        .map_err(|e| Status::failed_precondition(e.to_string()))?;

                Ok(quorumed_aggregation)
            })
            .await
            .map_err(|e: Arc<Status>| (*e).clone())?;

        let state_attestation = self
            .state_cache
            .try_get_with(packet_attestation.height, async {
                let state_attestations = self
                    .query_attestations(AttestationQuery::State(packet_attestation.height))
                    .await?;

                let quorumed_aggregation =
                    agg_quorumed_attestations(self.quorum_threshold, state_attestations)
                        .map_err(|e| Status::failed_precondition(e.to_string()))?;

                Ok(quorumed_aggregation)
            })
            .await
            .map_err(|e: Arc<Status>| (*e).clone())?;

        let response = GetAttestationsResponse {
            state_attestation: Some(state_attestation),
            packet_attestation: Some(packet_attestation),
        };
        Ok(Response::new(response))
    }
}

impl Aggregator {
    async fn query_attestations(
        &self,
        query: AttestationQuery,
    ) -> Result<Vec<Option<Attestation>>, Status> {
        let timeout_duration = self.attestor_timeout_duration;
        let query_futures = self.attestor_clients.iter().map(|(endpoint, client)| {
            let query = query.clone();
            async move {
                let mut client = client.lock().await;
                let response = timeout(timeout_duration, async {
                    match query {
                        AttestationQuery::Packet(packets, height) => {
                            let request =
                                Request::new(PacketAttestationRequest { packets, height });
                            client
                                .packet_attestation(request)
                                .await
                                .map(|r| r.into_inner().attestation)
                        }
                        AttestationQuery::State(height) => {
                            let request = Request::new(StateAttestationRequest { height });
                            client
                                .state_attestation(request)
                                .await
                                .map(|r| r.into_inner().attestation)
                        }
                    }
                })
                .await;

                let result = match response {
                    Ok(Ok(attestation)) => Ok(attestation),
                    Ok(Err(status)) => Err(status),
                    Err(_) => Err(Status::deadline_exceeded(format!(
                        "Request timed out after {timeout_duration:?}"
                    ))),
                };
                (endpoint, result)
            }
        });

        let results = join_all(query_futures).await;

        let successful_responses = results
            .into_iter()
            .filter_map(|(endpoint, result)| match result {
                Ok(attestation) => Some(attestation),
                Err(e) => {
                    tracing_error!("Attestor [{endpoint}] failed, error: {e:?}");
                    None
                }
            })
            .collect();

        Ok(successful_responses)
    }

    fn make_packet_cache_key(packets: &[Vec<u8>], height: u64) -> ([u8; 32], u64) {
        let mut hasher = Sha256::new();
        packets.iter().for_each(|p| hasher.update(p));
        (hasher.finalize().into(), height)
    }
}

/// Process attestations and create an aggregate response if the quorum is met.
fn agg_quorumed_attestations(
    quorum_threshold: usize,
    attestations: Vec<Option<Attestation>>,
) -> Result<AggregatedAttestation, anyhow::Error> {
    let mut attestator_data = AttestatorData::new();

    attestations.into_iter().flatten().for_each(|attestation| {
        if let Err(e) = attestator_data.insert(attestation) {
            tracing_error!("Invalid attestation, continuing with other responses: {e:#?}");
        }
    });

    attestator_data
        .agg_quorumed_attestations(quorum_threshold)
        .ok_or(anyhow::anyhow!("Quorum not met"))
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::{
        config::{AttestorConfig, Config, ServerConfig},
        mock_attestor::setup_attestor_server,
    };
    use std::net::SocketAddr;

    fn default_config(
        timeout: u64,
        quorum_threshold: usize,
        attestor_endpoints: Vec<SocketAddr>,
    ) -> Config {
        Config {
            server: ServerConfig {
                listener_addr: "127.0.0.1:8080".parse().unwrap(),
                log_level: "INFO".to_string(),
            },
            attestor: AttestorConfig {
                attestor_query_timeout_ms: timeout,
                quorum_threshold,
                attestor_endpoints: attestor_endpoints
                    .into_iter()
                    .map(|s| format!("http://{s}"))
                    .collect(),
            },
            cache: Default::default(),
        }
    }

    #[tokio::test]
    async fn get_aggregate_attestation_quorum_met() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 malicious attestor.
        let (addr_1, pk_1) = setup_attestor_server(false, 0, 1).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0, 2).await.unwrap();
        let (addr_3, pk_3) = setup_attestor_server(false, 0, 3).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0, 4).await.unwrap(); // This one is malicious

        // 2. Setup: Create AggregatorService
        let config = default_config(5000, 3, vec![addr_1, addr_2, addr_3, addr_4]);
        let aggregator_service = Aggregator::from_config(config).await.unwrap();

        // 3. Execute: Query for an aggregated attestation
        let response = aggregator_service
            .query_attestations(AttestationQuery::State(110))
            .await
            .unwrap();

        // 4. Assert: Check the response
        assert_eq!(response.len(), 4);
        assert!(response
            .iter()
            .any(|r| r.as_ref().unwrap().public_key == pk_1));
        assert!(response
            .iter()
            .any(|r| r.as_ref().unwrap().public_key == pk_2));
        assert!(response
            .iter()
            .any(|r| r.as_ref().unwrap().public_key == pk_3));
    }

    #[tokio::test]
    async fn get_aggregate_attestation_network_timeout() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 malicious attestor.
        // But successful attestors will be timeouted.
        let (addr_1, _) = setup_attestor_server(false, 1000, 1).await.unwrap();
        let (addr_2, _) = setup_attestor_server(false, 1000, 2).await.unwrap();
        let (addr_3, _) = setup_attestor_server(false, 1000, 3).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0, 4).await.unwrap(); // This one is malicious

        // 2. Setup: Create AggregatorService
        let config = default_config(100, 3, vec![addr_1, addr_2, addr_3, addr_4]);
        let aggregator_service = Aggregator::from_config(config).await.unwrap();

        // 3. Execute: Query for an aggregated attestation
        let response = aggregator_service
            .query_attestations(AttestationQuery::State(100))
            .await;

        // 4. Assert: Can not reach quorum due to timeouts
        let response = response.unwrap();
        assert_eq!(response.len(), 1);
    }
}
