use crate::{
    attestor_data::AttestatorData,
    config::AttestorConfig,
    error::{AggregatorError, IntoAggregatorError, Result},
    rpc::{
        aggregator_server::Aggregator, attestation_service_client::AttestationServiceClient,
        AggregateRequest, AggregateResponse, AttestationsFromHeightRequest, AttestationsFromHeightResponse,
    },
};

use futures::{stream::FuturesUnordered, StreamExt};
use tokio::{
    sync::{Mutex, RwLock},
    time::{timeout, Duration},
};
use tonic::{transport::Channel, Request, Response, Status};

#[derive(Debug)]
pub struct AggregatorService {
    config: AttestorConfig,
    attestor_clients: Vec<Mutex<AttestationServiceClient<Channel>>>,
    cached_height: RwLock<AggregateResponse>,
}

impl AggregatorService {
    pub async fn from_config(config: AttestorConfig) -> Result<Self> {
        let mut attestor_clients = Vec::new();

        for endpoint in &config.attestor_endpoints {
            let client = AttestationServiceClient::connect(endpoint.to_string())
                .await
                .map_err(|e| AggregatorError::attestor_connection(
                    endpoint,
                    "Failed to establish initial connection",
                    Some(e),
                ))?;

            attestor_clients.push(Mutex::new(client));
        }

        Ok(Self {
            config,
            attestor_clients,
            cached_height: RwLock::new(AggregateResponse {
                height: 0,
                state: vec![],
                sig_pubkey_pairs: vec![],
            }),
        })
    }
}

#[tonic::async_trait]
impl Aggregator for AggregatorService {
    #[tracing::instrument(skip_all, fields(min_height = request.get_ref().min_height))]
    async fn get_aggregate_attestation(
        &self,
        request: Request<AggregateRequest>,
    ) -> std::result::Result<Response<AggregateResponse>, Status> {
        let min_height = request.into_inner().min_height;

        // Check cache first
        {
            let cached_height = self.cached_height.read().await;
            if min_height <= cached_height.height {
                return Ok(Response::new(cached_height.clone()));
            }
        }

        let responses = self.query_all_attestors(min_height).await
            .map_err(|e| e.to_grpc_status())?;

        let aggregate_response = self.process_attestor_responses(responses).await
            .map_err(|e| e.to_grpc_status())?;

        if aggregate_response.height < min_height {
            return Err(AggregatorError::no_attestations_found(min_height).to_grpc_status());
        }

        // Update cache if we have a newer height
        {
            let mut cached_height = self.cached_height.write().await;
            *cached_height = aggregate_response.clone();
        }

        Ok(Response::new(aggregate_response))
    }
}

impl AggregatorService {
    /// Query all attestors concurrently and collect successful responses
    async fn query_all_attestors(&self, min_height: u64) -> Result<Vec<AttestationsFromHeightResponse>> {
        let mut futs = FuturesUnordered::new();
        let timeout_duration = Duration::from_millis(self.config.attestor_query_timeout_ms);

        // Create futures for all attestor queries
        for client in self.attestor_clients.iter() {
            let mut client = client.lock().await;
            let req = Request::new(AttestationsFromHeightRequest { height: min_height });

            futs.push(async move {
                match timeout(timeout_duration, client.get_attestations_from_height(req)).await {
                    Ok(Ok(resp)) => Ok(resp.into_inner()),
                    Ok(Err(status)) => Err(status.into_aggregator_error()),
                    Err(_) => Err(AggregatorError::timeout(self.config.attestor_query_timeout_ms)),
                }
            });
        }

        let mut successful_responses = Vec::new();
        let mut error_count = 0;

        // Collect all responses
        while let Some(result) = futs.next().await {
            match result {
                Ok(response) => successful_responses.push(response),
                Err(e) => {
                    error_count += 1;
                    tracing::error!("Attestor query failed: {e}");
                }
            }
        }

        if successful_responses.is_empty() {
            return Err(AggregatorError::internal(format!(
                "No attestors responded successfully ({error_count} errors)",
            )));
        }

        Ok(successful_responses)
    }

    /// Process attestor responses and create an aggregate response
    async fn process_attestor_responses(&self, responses: Vec<AttestationsFromHeightResponse>) -> Result<AggregateResponse> {
        let mut attestator_data = AttestatorData::new();

        // Insert all responses into the data aggregator
        for response in responses {
            attestator_data.insert(response);
        }

        // Get the latest aggregate response that meets quorum
        match attestator_data.get_latest(self.config.quorum_threshold) {
            Some(aggregate_response) => Ok(aggregate_response),
            None => Err(AggregatorError::quorum_not_met(self.config.quorum_threshold)),
        }
    }
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::{
        config::{AttestorConfig, Config, ServerConfig},
        mock_attestor::setup_attestor_server,
    };

    fn default_config(timeout: u64, attestor_endpoints: Vec<String>) -> Config {
        Config {
            server: ServerConfig {
                listener_addr: "127.0.0.1:50060".parse().unwrap(),
                log_level: "INFO".to_string(),
            },
            attestor: AttestorConfig {
                attestor_query_timeout_ms: timeout,
                quorum_threshold: 3,
                attestor_endpoints,
            },
        }
    }

    #[tokio::test]
    async fn get_aggregate_attestation_quorum_met() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 failing attestor.
        let (addr_1, pk_1) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_3, pk_3) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0).await.unwrap(); // This one will fail

        // 2. Setup: Create AggregatorService
        let config = default_config(
            5000,
            vec![
                format!("http://{addr_1}"),
                format!("http://{addr_2}"),
                format!("http://{addr_3}"),
                format!("http://{addr_4}"),
            ],
        );

        let aggregator_service = AggregatorService::from_config(config.attestor).await.unwrap();

        // 3. Execute: Query for an aggregated attestation
        let request = Request::new(AggregateRequest { min_height: 100 });
        let response = aggregator_service
            .get_aggregate_attestation(request)
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

        assert_eq!(aggres.state.len(), 12); // Assuming state is 12 bytes long
    }

    #[tokio::test]
    async fn get_aggregate_attestation_network_timeout() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 failing attestor.
        let (addr_1, _) = setup_attestor_server(false, 1000).await.unwrap();
        let (addr_2, _) = setup_attestor_server(false, 1000).await.unwrap();
        let (addr_3, _) = setup_attestor_server(false, 1000).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0).await.unwrap(); // This one will fail

        // 2. Setup: Create AggregatorService
        let config = default_config(
            100,
            vec![
                format!("http://{addr_1}"),
                format!("http://{addr_2}"),
                format!("http://{addr_3}"),
                format!("http://{addr_4}"),
            ],
        );

        let aggregator_service = AggregatorService::from_config(config.attestor).await.unwrap();

        // 3. Execute: Query for an aggregated attestation
        let request = Request::new(AggregateRequest { min_height: 100 });
        let response = aggregator_service.get_aggregate_attestation(request).await;

        // 4. Assert: Can not reach quorum due to timeouts
        assert!(response.is_err());
        let status = response.unwrap_err();
        assert_eq!(status.code(), tonic::Code::Internal);
        assert!(status.message().contains("No attestors responded successfully"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attestor_data::{PUBKEY_BYTE_LENGTH, SIGNATURE_BYTE_LENGTH, STATE_BYTE_LENGTH};
    use crate::rpc::{AttestationEntry, AttestationsFromHeightResponse};

    // Helper to build a FixedBytes-N vector filled with `b`
    fn fill_bytes<const N: usize>(b: u8) -> Vec<u8> {
        vec![b; N]
    }

    #[test]
    fn ignores_states_below_quorum() {
        // We have a height 100 but only 1 signature < quorum 2
        let mut attestator_data = AttestatorData::new();

        attestator_data.insert(AttestationsFromHeightResponse {
            pubkey: fill_bytes::<PUBKEY_BYTE_LENGTH>(0x03),
            attestations: vec![AttestationEntry {
                height: 100,
                data: fill_bytes::<STATE_BYTE_LENGTH>(1),
                signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(0x04),
            }],
        });

        let latest = attestator_data.get_latest(2); // Quprum 2
        assert!(latest.is_none(), "Should not return a state below quorum");
    }

    #[test]
    fn picks_single_height_meeting_quorum() {
        let mut attestator_data = AttestatorData::new();
        let state = fill_bytes::<STATE_BYTE_LENGTH>(0xAA);
        attestator_data.insert(AttestationsFromHeightResponse {
            pubkey: fill_bytes::<PUBKEY_BYTE_LENGTH>(0x21),
            attestations: vec![AttestationEntry {
                height: 123,
                data: state.clone(),
                signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(0x11),
            }],
        });

        attestator_data.insert(AttestationsFromHeightResponse {
            pubkey: fill_bytes::<PUBKEY_BYTE_LENGTH>(0x22),
            attestations: vec![AttestationEntry {
                height: 123,
                data: state.clone(),
                signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(0x11),
            }],
        });

        let latest = attestator_data.get_latest(2); // Quorum 2
        assert!(latest.is_some(), "Should return a state meeting quorum");
        let latest = latest.unwrap();
        assert_eq!(latest.height, 123);
        assert_eq!(latest.state, state);
        // Should have two SigPubkeyPair entries
        assert_eq!(latest.sig_pubkey_pairs.len(), 2);
        // Check that the pairs contain the pubkeys we inserted
        let pubs: Vec<_> = latest
            .sig_pubkey_pairs
            .into_iter()
            .map(|p| p.pubkey)
            .collect();
        assert!(pubs.contains(&fill_bytes::<PUBKEY_BYTE_LENGTH>(0x21)));
        assert!(pubs.contains(&fill_bytes::<PUBKEY_BYTE_LENGTH>(0x22)));
    }

    #[test]
    fn chooses_highest_height_when_multiple() {
        let mut attestator_data = AttestatorData::new();
        let state120 = fill_bytes::<STATE_BYTE_LENGTH>(0xAA);
        let state150 = fill_bytes::<STATE_BYTE_LENGTH>(0xBB);
        let state200 = fill_bytes::<STATE_BYTE_LENGTH>(0xCC);
        let pk_a = fill_bytes::<PUBKEY_BYTE_LENGTH>(0xA);
        let pk_b = fill_bytes::<PUBKEY_BYTE_LENGTH>(0xB);
        let pk_c = fill_bytes::<PUBKEY_BYTE_LENGTH>(0xC);

        attestator_data.insert(AttestationsFromHeightResponse {
            pubkey: pk_a.clone(),
            attestations: vec![
                AttestationEntry {
                    height: 120,
                    data: state120.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(1),
                },
                AttestationEntry {
                    height: 150,
                    data: state150.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(3),
                },
                AttestationEntry {
                    height: 200,
                    data: state200.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(5),
                },
            ],
        });

        attestator_data.insert(AttestationsFromHeightResponse {
            pubkey: pk_b.clone(),
            attestations: vec![
                AttestationEntry {
                    height: 120,
                    data: state120.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(2),
                },
                AttestationEntry {
                    height: 150,
                    data: state150.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(4),
                },
                AttestationEntry {
                    height: 200,
                    data: state200.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(6),
                },
            ],
        });

        attestator_data.insert(AttestationsFromHeightResponse {
            pubkey: pk_c.clone(),
            attestations: vec![
                AttestationEntry {
                    height: 120,
                    data: state120.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(5),
                },
                AttestationEntry {
                    height: 150,
                    data: state150.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(10),
                },
                AttestationEntry {
                    height: 200,
                    data: state200.clone(),
                    signature: fill_bytes::<SIGNATURE_BYTE_LENGTH>(15),
                },
            ],
        });

        let latest = attestator_data.get_latest(2); // Quorum 2
        assert!(latest.is_some(), "Should return a state meeting quorum");
        let latest = latest.unwrap();
        assert_eq!(latest.height, 200);
        assert_eq!(latest.state, state200);
        // Should have three SigPubkeyPair entries
        assert_eq!(latest.sig_pubkey_pairs.len(), 3);
        // Check that the pairs contain the pubkeys we inserted
        let pks: Vec<_> = latest
            .sig_pubkey_pairs
            .into_iter()
            .map(|p| p.pubkey)
            .collect();
        assert!(pks.contains(pk_a.as_ref()));
        assert!(pks.contains(pk_b.as_ref()));
        assert!(pks.contains(pk_c.as_ref()));
    }
}
