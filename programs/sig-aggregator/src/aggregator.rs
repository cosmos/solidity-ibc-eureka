use crate::{
    attestor_data::AttestatorData,
    config::{AttestorConfig, Config},
    rpc::{
        aggregator_service_server::AggregatorService,
        attestation_service_client::AttestationServiceClient, Attestation,
        GetStateAttestationRequest, GetStateAttestationResponse, PacketAttestationRequest,
        PacketAttestationResponse, StateAttestationRequest, StateAttestationResponse,
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
use tracing::error as tracing_error;

pub type AggregatedAttestation = GetStateAttestationResponse;

#[derive(Clone)]
enum AttestationQuery {
    Packet(Vec<Vec<u8>>),
    State(u64),
}

enum AttestationResponse {
    Packet(PacketAttestationResponse),
    State(StateAttestationResponse),
}

impl TryFrom<AttestationResponse> for PacketAttestationResponse {
    type Error = &'static str;

    fn try_from(value: AttestationResponse) -> Result<Self, Self::Error> {
        match value {
            AttestationResponse::Packet(response) => Ok(response),
            AttestationResponse::State(_) => {
                Err("Expected packet attestation, got state attestation")
            }
        }
    }
}

impl TryFrom<AttestationResponse> for StateAttestationResponse {
    type Error = &'static str;

    fn try_from(value: AttestationResponse) -> Result<Self, Self::Error> {
        match value {
            AttestationResponse::State(response) => Ok(response),
            AttestationResponse::Packet(_) => {
                Err("Expected state attestation, got packet attestation")
            }
        }
    }
}

#[derive(Debug)]
pub struct Aggregator {
    attestor_config: Arc<AttestorConfig>,
    attestor_clients: Vec<Mutex<AttestationServiceClient<Channel>>>,
    state_cache: Cache<u64, AggregatedAttestation>,
    packet_cache: Cache<Vec<u8>, AggregatedAttestation>,
}

impl Aggregator {
    pub async fn from_config(config: Config) -> anyhow::Result<Self> {
        let attestor_clients = Self::create_clients(&config.attestor).await?;

        Ok(Self {
            attestor_config: Arc::new(config.attestor),
            attestor_clients,
            state_cache: Cache::new(config.cache.state_cache_capacity),
            packet_cache: Cache::new(config.cache.packet_cache_capacity),
        })
    }

    async fn create_clients(
        config: &AttestorConfig,
    ) -> anyhow::Result<Vec<Mutex<AttestationServiceClient<Channel>>>> {
        let futures = config.attestor_endpoints.iter().map(|endpoint| async move {
            AttestationServiceClient::connect(endpoint.clone())
                .await
                .map(Mutex::new)
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
    async fn get_state_attestation(
        &self,
        request: Request<GetStateAttestationRequest>,
    ) -> Result<Response<AggregatedAttestation>, Status> {
        let mut packets = request.into_inner().packets;
        packets.sort();

        let packet_cache_key = Self::make_packet_cache_key(&packets);

        let packet_agg = self
            .packet_cache
            .try_get_with(packet_cache_key, async {
                let packet_attestations = self.query_packet_attestations(packets).await?;

                let quorumed_aggregation = Self::agg_quorumed_attestations(
                    self.attestor_config.quorum_threshold,
                    packet_attestations
                        .into_iter()
                        .map(|r| r.attestation)
                        .collect(),
                )
                .await?;

                Ok(quorumed_aggregation)
            })
            .await
            .map_err(|e: Arc<Status>| (*e).clone())?;

        let state_attestations = self
            .state_cache
            .try_get_with(packet_agg.height, async {
                let state_attestations = self.query_state_attestations(packet_agg.height).await?;
                let quorumed_aggregation = Self::agg_quorumed_attestations(
                    self.attestor_config.quorum_threshold,
                    state_attestations
                        .into_iter()
                        .map(|r| r.attestation)
                        .collect(),
                )
                .await?;

                Ok(quorumed_aggregation)
            })
            .await
            .map_err(|e: Arc<Status>| (*e).clone())?;

        Ok(Response::new(state_attestations))
    }
}

impl Aggregator {
    async fn query_packet_attestations(
        &self,
        packets: Vec<Vec<u8>>,
    ) -> Result<Vec<PacketAttestationResponse>, Status> {
        self.query_attestations(AttestationQuery::Packet(packets))
            .await
    }

    async fn query_state_attestations(
        &self,
        height: u64,
    ) -> Result<Vec<StateAttestationResponse>, Status> {
        self.query_attestations(AttestationQuery::State(height))
            .await
    }

    async fn query_attestations<T>(&self, query: AttestationQuery) -> Result<Vec<T>, Status>
    where
        T: TryFrom<AttestationResponse> + Send + 'static,
        T::Error: std::fmt::Debug,
    {
        let timeout_duration =
            Duration::from_millis(self.attestor_config.attestor_query_timeout_ms);

        let query_futures = self.attestor_clients.iter().enumerate().map(|(i, client)| {
            let endpoint = &self.attestor_config.attestor_endpoints[i];
            let query = query.clone();

            async move {
                let mut client = client.lock().await;
                let response = timeout(timeout_duration, async {
                    match query {
                        AttestationQuery::Packet(packets) => {
                            let request = Request::new(PacketAttestationRequest { packets });
                            client
                                .packet_attestation(request)
                                .await
                                .map(|r| AttestationResponse::Packet(r.into_inner()))
                        }
                        AttestationQuery::State(height) => {
                            let request = Request::new(StateAttestationRequest { height });
                            client
                                .state_attestation(request)
                                .await
                                .map(|r| AttestationResponse::State(r.into_inner()))
                        }
                    }
                })
                .await;

                let result = match response {
                    Ok(Ok(response)) => match T::try_from(response) {
                        Ok(converted) => Ok(converted),
                        Err(e) => Err(Status::internal(format!("Conversion failed: {e:?}"))),
                    },
                    Ok(Err(status)) => Err(status),
                    Err(e) => Err(Status::deadline_exceeded(format!(
                        "Request timed out after {timeout_duration:?} Elapsed: {e:?}"
                    ))),
                };
                (endpoint, result)
            }
        });

        let results = join_all(query_futures).await;

        let successful_responses = results
            .into_iter()
            .filter_map(|(endpoint, result)| match result {
                Ok(response) => Some(response),
                Err(e) => {
                    tracing_error!("Attestor [{endpoint}] failed, error: {e:?}");
                    None
                }
            })
            .collect();

        Ok(successful_responses)
    }

    fn make_packet_cache_key(packets: &[Vec<u8>]) -> Vec<u8> {
        let mut concatenated = Vec::new();
        for packet in packets {
            let mut hasher = Sha256::new();
            hasher.update(packet);
            concatenated.extend_from_slice(&hasher.finalize());
        }
        concatenated
    }

    /// Process attestations and create an aggregate response if the quorum is met.
    async fn agg_quorumed_attestations(
        quorum_threshold: usize,
        attestations: Vec<Option<Attestation>>,
    ) -> Result<AggregatedAttestation, Status> {
        let mut attestator_data = AttestatorData::new();

        attestations.into_iter().flatten().for_each(|attestation| {
            if let Err(e) = attestator_data.insert(attestation) {
                tracing_error!("Invalid attestation, continuing with other responses: {e:#?}");
            }
        });

        attestator_data
            .agg_quorumed_attestations(quorum_threshold)
            .ok_or(Status::failed_precondition("Quorum not met"))
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::{
        attestor_data::STATE_BYTE_LENGTH,
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
            .query_state_attestations(110)
            .await
            .unwrap();

        // 4. Assert: Check the response
        assert_eq!(response.len(), 4);
        assert!(response
            .iter()
            .any(|r| r.attestation.as_ref().unwrap().public_key == pk_1));
        assert!(response
            .iter()
            .any(|r| r.attestation.as_ref().unwrap().public_key == pk_2));
        assert!(response
            .iter()
            .any(|r| r.attestation.as_ref().unwrap().public_key == pk_3));

        assert!(response
            .iter()
            .all(|r| r.attestation.as_ref().unwrap().attested_data.len() == STATE_BYTE_LENGTH));
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
        let response = aggregator_service.query_state_attestations(100).await;

        // 4. Assert: Can not reach quorum due to timeouts
        let response = response.unwrap();
        assert_eq!(response.len(), 1);
    }
}
