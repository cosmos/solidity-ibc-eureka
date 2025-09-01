//! Reusable aggregator for attester relayer modules

#[allow(clippy::all, clippy::pedantic, clippy::nursery)]
/// RPC definitions for the attestor
pub mod rpc {
    tonic::include_proto!("aggregator");
    tonic::include_proto!("ibc_attestor");
}

mod attestor_data;
mod config;

use futures::future::join_all;
use moka::future::Cache;
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tokio::{
    sync::Mutex,
    time::{timeout, Duration},
};
use tonic::{transport::Channel, Request, Status};
use tracing::error as tracing_error;
use {
    attestor_data::AttestatorData,
    rpc::{
        attestation_service_client::AttestationServiceClient, AggregatedAttestation, Attestation,
        PacketAttestationRequest, StateAttestationRequest,
    },
};

pub use config::{AttestorConfig, CacheConfig, Config};

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
    /// Creates aggregator from the [`Config`]
    ///
    /// # Errors
    /// - Fails if clients cannot be created
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

impl Aggregator {
    /// Get attestations
    ///
    /// # Errors
    /// - Fails if attesations cannot be fetched
    /// - Fails if input packets are empty
    pub async fn get_attestations(
        &self,
        packets: Vec<Vec<u8>>,
        height: u64,
    ) -> anyhow::Result<(AggregatedAttestation, AggregatedAttestation)> {
        if packets.is_empty() {
            return Err(anyhow::anyhow!("Packets cannot be empty"));
        }

        if packets.iter().any(std::vec::Vec::is_empty) {
            return Err(anyhow::anyhow!("Packet cannot be empty"));
        }

        let mut sorted_packets = packets;
        sorted_packets.sort();
        let packet_cache_key = Self::make_packet_cache_key(&sorted_packets, height);

        let packet_attestation = self
            .packet_cache
            .try_get_with(packet_cache_key, async {
                let packet_attestations = self
                    .query_attestations(AttestationQuery::Packet(sorted_packets, height))
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

        Ok((state_attestation, packet_attestation))
    }

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
        for p in packets {
            hasher.update(p);
        }
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
        .ok_or_else(|| anyhow::anyhow!("Quorum not met"))
}
