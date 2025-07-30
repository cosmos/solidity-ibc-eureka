use crate::{
    attestor_data::AttestatorData,
    config::AttestorConfig,
    rpc::{
        aggregator_service_server::AggregatorService,
        attestation_service_client::AttestationServiceClient, Attestation,
        GetStateAttestationRequest, GetStateAttestationResponse, PacketAttestationRequest,
        PacketAttestationResponse, StateAttestationRequest, StateAttestationResponse,
    },
};
use futures::future::join_all;
use std::sync::Arc;
use tokio::{
    sync::{Mutex, RwLock},
    time::{timeout, Duration},
};
use tonic::{transport::Channel, Request, Response, Status};
use tracing::error as tracing_error;

#[derive(Debug)]
pub struct Aggregator {
    attestor_config: Arc<AttestorConfig>,
    attestor_clients: Vec<Mutex<AttestationServiceClient<Channel>>>,
    state_cache: RwLock<Option<GetStateAttestationResponse>>,
}

impl Aggregator {
    pub async fn from_attestor_config(attestor_config: AttestorConfig) -> anyhow::Result<Self> {
        let attestor_clients = Self::create_clients(&attestor_config).await?;

        Ok(Self {
            attestor_config: Arc::new(attestor_config),
            attestor_clients,
            state_cache: RwLock::new(None),
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

    async fn get_cached_response(&self, height: u64) -> Option<GetStateAttestationResponse> {
        self.state_cache
            .read()
            .await
            .as_ref()
            .filter(|response| response.height >= height)
            .cloned()
    }

    async fn update_cache(&self, new_resp: &GetStateAttestationResponse) {
        let mut cached = self.state_cache.write().await;
        if cached.as_ref().is_none_or(|c| new_resp.height > c.height) {
            *cached = Some(new_resp.clone());
        }
    }
}

#[tonic::async_trait]
impl AggregatorService for Aggregator {
    async fn get_state_attestation(
        &self,
        request: Request<GetStateAttestationRequest>,
    ) -> Result<Response<GetStateAttestationResponse>, Status> {
        let packet_attestations = self
            .get_packet_attestations(request.into_inner().packets)
            .await?;

        let quorum = get_quorum(
            self.attestor_config.quorum_threshold,
            packet_attestations
                .into_iter()
                .map(|r| Box::new(r) as Box<dyn ContainsAttestation + Send>)
                .collect(),
        )
        .await?;

        let state = self
            .get_aggregate_attestation(quorum.height)
            .await?
            .into_inner();

        Ok(Response::new(GetStateAttestationResponse {
            sig_pubkey_pairs: state.sig_pubkey_pairs,
            state: state.state,
            height: state.height,
        }))
    }
}

impl Aggregator {
    async fn get_aggregate_attestation(
        &self,
        height: u64,
    ) -> std::result::Result<Response<GetStateAttestationResponse>, Status> {
        // Check cache first
        if let Some(cached_response) = self.get_cached_response(height).await {
            return Ok(Response::new(cached_response));
        }

        let responses = self.query_all_attestors(height).await?;
        let aggregate_response = get_quorum(
            self.attestor_config.quorum_threshold,
            responses
                .into_iter()
                .map(|r| Box::new(r) as Box<dyn ContainsAttestation + Send>)
                .collect(),
        )
        .await?;

        if aggregate_response.height < height {
            return Err(Status::not_found(format!(
                "No valid attestations found for height >= {height}"
            )));
        }

        self.update_cache(&aggregate_response).await;
        Ok(Response::new(aggregate_response))
    }

    async fn get_packet_attestations(
        &self,
        packets: Vec<Vec<u8>>,
    ) -> Result<Vec<PacketAttestationResponse>, Status> {
        let timeout_duration =
            Duration::from_millis(self.attestor_config.attestor_query_timeout_ms);

        let query_futures = self.attestor_clients.iter().enumerate().map(|(i, client)| {
            let endpoint = &self.attestor_config.attestor_endpoints[i];
            let packets = packets.clone();
            let request = Request::new(PacketAttestationRequest { packets });

            async move {
                let mut client = client.lock().await;
                let response = timeout(timeout_duration, client.packet_attestation(request)).await;

                let result = match response {
                    Ok(Ok(response)) => Ok(response.into_inner()),
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
                Ok(response) => Some(response),
                Err(e) => {
                    tracing_error!(
                        "Attestor [{endpoint}] failed, continuing with other responses: {e:?}"
                    );
                    None
                }
            })
            .collect();

        Ok(successful_responses)
    }
}

impl Aggregator {
    /// Query all attestors concurrently and collect successful responses
    async fn query_all_attestors(
        &self,
        min_height: u64,
    ) -> Result<Vec<StateAttestationResponse>, Status> {
        let timeout_duration =
            Duration::from_millis(self.attestor_config.attestor_query_timeout_ms);

        let query_futures = self.attestor_clients.iter().enumerate().map(|(i, client)| {
            let endpoint = &self.attestor_config.attestor_endpoints[i];

            async move {
                let mut client = client.lock().await;
                let request = Request::new(StateAttestationRequest { height: min_height });
                let response = timeout(timeout_duration, client.state_attestation(request)).await;

                let result = match response {
                    Ok(Ok(response)) => Ok(response.into_inner()),
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
                Ok(response) => Some(response),
                Err(e) => {
                    tracing_error!(
                        "Attestor [{endpoint}] failed, continuing with other responses: {e:?}"
                    );
                    None
                }
            })
            .collect();

        Ok(successful_responses)
    }
}

/// Process attestor responses and create an aggregate response where the quorum is met.
async fn get_quorum(
    quorum_threshold: usize,
    responses: Vec<Box<dyn ContainsAttestation + Send>>,
) -> Result<GetStateAttestationResponse, Status> {
    let mut attestator_data = AttestatorData::new();

    responses
        .into_iter()
        .filter_map(|response| response.get_attestation())
        .for_each(|attestation| {
            if let Err(e) = attestator_data.insert(attestation) {
                tracing_error!("Invalid attestation, continuing with other responses: {e:#?}");
            }
        });

        attestator_data
            .agg_quorumed_attestations(self.attestor_config.quorum_threshold)
            .ok_or(Status::failed_precondition("Quorum not met"))
    }
}

trait ContainsAttestation {
    fn get_attestation(&self) -> Option<Attestation>;
}

impl ContainsAttestation for StateAttestationResponse {
    fn get_attestation(&self) -> Option<Attestation> {
        self.attestation.clone()
    }
}

impl ContainsAttestation for PacketAttestationResponse {
    fn get_attestation(&self) -> Option<Attestation> {
        self.attestation.clone()
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::{
        attestor_data::STATE_BYTE_LENGTH, config::AttestorConfig,
        mock_attestor::setup_attestor_server,
    };

    fn default_attestor_config(timeout: u64, attestor_endpoints: Vec<String>) -> AttestorConfig {
        AttestorConfig {
            attestor_query_timeout_ms: timeout,
            quorum_threshold: 3,
            attestor_endpoints,
        }
    }

    #[tokio::test]
    async fn get_aggregate_attestation_quorum_met() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 failing attestor.
        let (addr_1, pk_1) = setup_attestor_server(false, 0, 1).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0, 2).await.unwrap();
        let (addr_3, pk_3) = setup_attestor_server(false, 0, 3).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0, 4).await.unwrap(); // This one is malicious

        // 2. Setup: Create AggregatorService
        let config = default_attestor_config(
            5000,
            vec![
                format!("http://{addr_1}"),
                format!("http://{addr_2}"),
                format!("http://{addr_3}"),
                format!("http://{addr_4}"),
            ],
        );

        let aggregator_service = Aggregator::from_attestor_config(config).await.unwrap();

        // 3. Execute: Query for an aggregated attestation
        let response = aggregator_service
            .get_aggregate_attestation(110)
            .await
            .unwrap();

        // 4. Assert: Check the response
        let aggres = response.into_inner();
        assert_eq!(aggres.height, 110);
        assert_eq!(aggres.sig_pubkey_pairs.len(), 3);
        assert!(aggres
            .sig_pubkey_pairs
            .iter()
            .any(|pair| pair.pubkey == pk_1));
        assert!(aggres
            .sig_pubkey_pairs
            .iter()
            .any(|pair| pair.pubkey == pk_2));
        assert!(aggres
            .sig_pubkey_pairs
            .iter()
            .any(|pair| pair.pubkey == pk_3));

        assert_eq!(aggres.state.len(), STATE_BYTE_LENGTH);
    }

    #[tokio::test]
    async fn get_aggregate_attestation_network_timeout() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 malicious attestor.
        let (addr_1, _) = setup_attestor_server(false, 1000, 1).await.unwrap();
        let (addr_2, _) = setup_attestor_server(false, 1000, 2).await.unwrap();
        let (addr_3, _) = setup_attestor_server(false, 1000, 3).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0, 4).await.unwrap(); // This one is malicious

        // 2. Setup: Create AggregatorService
        let config = default_attestor_config(
            100,
            vec![
                format!("http://{addr_1}"),
                format!("http://{addr_2}"),
                format!("http://{addr_3}"),
                format!("http://{addr_4}"),
            ],
        );

        let aggregator_service = Aggregator::from_attestor_config(config).await.unwrap();

        // 3. Execute: Query for an aggregated attestation
        let response = aggregator_service.get_aggregate_attestation(100).await;

        // 4. Assert: Can not reach quorum due to timeouts
        assert!(response.is_err());
        let status = response.unwrap_err();
        assert_eq!(status.code(), tonic::Code::FailedPrecondition);
        assert!(status.message().contains("Quorum not met"));
    }
}
