use tokio::{time::{timeout, Duration}, sync::{RwLock, mpsc}};
use tonic::{transport::Channel, Request, Response, Status};
use std::{sync::Arc, collections::HashMap};
use crate::{
    config::Config,
    error::AggregatorError,
    rpc::{
        aggregator_server::Aggregator,
        attestor_client::AttestorClient,
        AggregateRequest, AggregateResponse, QueryRequest, SigPubkeyPair,
    },
};

#[derive(Debug)]
pub struct AggregatorService {
    config: Config,
    attestor_clients: Vec<AttestorClient<Channel>>,
    cached_height: Arc<RwLock<AggregateResponse>>,
}

impl AggregatorService {
    pub async fn from_config(config: Config) -> Result<Self, AggregatorError> {
        let mut attestor_clients = Vec::new();
        for endpoint in &config.attestor_endpoints {
            let client = AttestorClient::connect(endpoint.clone())
                .await
                .map_err(|e| AggregatorError::Config(e.to_string()))?;
            attestor_clients.push(client);
        }
        Ok(Self {
            config,
            attestor_clients,
            cached_height: Arc::new(RwLock::new(AggregateResponse {
                height: 0,
                state: vec![],
                sig_pubkey_pairs: vec![],
            })),
        })
    }
}

// TODO: 1. FIx len data
// TODO: Proof that we don't need RwLock here.

#[tonic::async_trait]
impl Aggregator for AggregatorService {
    #[tracing::instrument(skip_all, fields(min_height = request.get_ref().min_height))]
    async fn get_aggregate_attestation(
        &self,
        request: Request<AggregateRequest>,
    ) -> Result<Response<AggregateResponse>, Status> {
        let min_height = request.into_inner().min_height;
        {
            let cached_height = self.cached_height.read().await;
            if min_height <= cached_height.height {
                return Ok(Response::new(cached_height.clone()));
            }
        }

        let (tx, mut rx) = mpsc::channel(self.attestor_clients.len());

        for mut client in self.attestor_clients.clone() {
            let tx = tx.clone();
            tokio::spawn(async move {
                let request = tonic::Request::new(QueryRequest { min_height });
                let result = client.query_attestations(request).await;
                if let Err(e) = tx.send(result).await {
                    tracing::error!("Failed to send attestor result to channel: {}", e);
                }
            });
        }
        drop(tx);
        
        let mut responses_received = 0;
        let attestor_query_timeout = Duration::from_millis(self.config.attestor_query_timeout_ms);
        
        //  HashMap<height, HashMap<State, Vec[(Signatures, pub_key)]>>
        //  Height: 101
        //      State: 0x1234... (32 bytes)
        //          Sign_PK: [(SigAtt_A, PK_Att_A), (SigAtt_B, PK_Att_B)]
        //      State: 0x9876...
        //          Sign_PK: [(SigAtt_C, PK_Att_C), (SigAtt_D, PK_Att_D), (SigAtt_E, PK_Att_E)]
        //  Height: 102
        //      State: 0x5678...
        //          Sign_PK: [(SigAtt_A, PK_Att_A), (SigAtt_B, PK_Att_B), (SigAtt_C, PK_Att_C), (SigAtt_D, PK_Att_D), (SigAtt_E, PK_Att_E)]
        let mut height_to_state: HashMap<u64, HashMap<Vec<u8>, Vec<(Vec<u8>, Vec<u8>)>>> = HashMap::new();
        
        let collection_result = timeout(attestor_query_timeout, async {
            while let Some(result) = rx.recv().await {
                responses_received += 1;
                match result {
                    Ok(response) => {
                        let attestations = response.into_inner();
                        for attestation in attestations.attestations {
                            let state_map = height_to_state.entry(attestation.height).or_default();
                            state_map
                                .entry(attestation.state)
                                .or_default()
                                .push((attestation.signature, attestations.pubkey.clone()));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("An attestor query failed: {}", e);
                    }
                }
            }
        }).await;
        
        if let Err(e) = collection_result {
            tracing::warn!("Attestor collection timed out after {:?}. Error: {:?}", attestor_query_timeout, e);
        }
        
        // Find the highest height with a quorum
        for (height, state_to_signatures) in height_to_state.iter() {
            // If we have more than one state at this height, raise some monitoring warning.
            if state_to_signatures.keys().len() > 1 {
                // TODO: Decide how to raise multiple states for a height.
                println!("multiple [{}] state found for height {}", state_to_signatures.keys().len(), height);
            }

            // Skip heights lower than the cached height
            {
                let cached_height = self.cached_height.read().await;
                if *height < cached_height.height {
                    continue; 
                }
            }

            for (state, sig_pks) in state_to_signatures.iter() {
                if sig_pks.len() < self.config.quorum_threshold {
                    continue;
                }

                let candidate = AggregateResponse {
                    height: *height,
                    state: state.clone(),
                    sig_pubkey_pairs: sig_pks
                        .iter()
                        .map(|(sig, pubkey)| SigPubkeyPair { sig: sig.clone(), pubkey: pubkey.clone() })
                        .collect(),
                };
                let mut cached_height = self.cached_height.write().await;
                *cached_height = candidate.clone();
            }
        }

        let cached_height = self.cached_height.read().await;
        Ok(Response::new(cached_height.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        attestor::MockAttestor,
        config::Config,
        rpc::attestor_server::{AttestorServer},
    };
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tonic::transport::Server;
    use tokio_stream;

    // Helper to spin up a mock attestor server on a random available port.
    // Returns the address it's listening on.
    async fn setup_attestor_server(should_fail: bool, delay_ms: u64) -> anyhow::Result<(SocketAddr, Vec<u8>)> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let attestor = MockAttestor::new(should_fail, delay_ms);
        let pubkey = attestor.get_pubkey();

        tokio::spawn(async move {
            Server::builder()
                .add_service(AttestorServer::new(attestor))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        Ok((addr, pubkey))
    }

    #[tokio::test]
    async fn test_get_aggregate_attestation_quorum_met() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 failing attestor.
        let (addr_1, pk_1) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_3, pk_3) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0).await.unwrap(); // This one will fail
        
        // 2. Setup: Create AggregatorService
        let config = Config {
            attestor_endpoints: vec![
                format!("http://{}", addr_1),
                format!("http://{}", addr_2),
                format!("http://{}", addr_3),
                format!("http://{}", addr_4),
            ],
            quorum_threshold: 3,
            listen_addr: "127.0.0.1:50060".to_string(), // Not used in this test
            attestor_query_timeout_ms: 5000,
        };

        let aggregator_service = AggregatorService::from_config(config).await.unwrap();

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
        assert!(aggres.sig_pubkey_pairs.iter().any(|pair| pair.pubkey == pk_1));
        assert!(aggres.sig_pubkey_pairs.iter().any(|pair| pair.pubkey == pk_2));
        assert!(aggres.sig_pubkey_pairs.iter().any(|pair| pair.pubkey == pk_3));

        assert_eq!(aggres.state.len(), 32); // Assuming state is 32 bytes long
    }
}