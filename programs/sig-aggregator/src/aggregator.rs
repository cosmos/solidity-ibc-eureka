use tokio::{time::{timeout, Duration}, sync::{RwLock, Mutex, mpsc}};
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
use alloy_primitives::{FixedBytes};

type Height = u64;
type State = FixedBytes<32>;
type Signature = FixedBytes<32>;
type Pubkey = FixedBytes<65>;
type SignedStates = HashMap<State, Vec<(Signature, Pubkey)>>;

#[derive(Debug)]
pub struct AggregatorService {
    config: Config,
    attestor_clients: Vec<Arc<Mutex<AttestorClient<Channel>>>>,
    cached_height: Arc<RwLock<AggregateResponse>>,
}

impl AggregatorService {
    pub async fn from_config(config: Config) -> Result<Self, AggregatorError> {
        let mut attestor_clients = Vec::new();
        for endpoint in &config.attestor_endpoints {
            let client = AttestorClient::connect(endpoint.to_string())
                .await
                .map_err(|e| AggregatorError::Config(e.to_string()))?;
            attestor_clients.push(Arc::new(Mutex::new(client)));
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


#[tonic::async_trait]
impl Aggregator for AggregatorService {
    #[tracing::instrument(skip_all, fields(min_height = request.get_ref().min_height))]
    async fn get_aggregate_attestation(
        &self,
        request: Request<AggregateRequest>,
    ) -> Result<Response<AggregateResponse>, Status> {
        let min_height = request.into_inner().min_height;
        let cached_height = self.cached_height.read().await;
        if min_height <= cached_height.height {
            return Ok(Response::new(cached_height.clone()));
        }
    
        let (tx, mut rx) = mpsc::channel(self.attestor_clients.len());

        for client_ref in &self.attestor_clients {
            let client_arc = Arc::clone(client_ref);
            let tx = tx.clone();
            tokio::spawn(async move {
                let mut client = client_arc.lock().await;
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
        let mut height_to_state: HashMap<Height, SignedStates> = HashMap::new();
        
        let collection_result = timeout(attestor_query_timeout, async {
            while let Some(result) = rx.recv().await {
                responses_received += 1;
                match result {
                    Ok(response) => {
                        let attestations = response.into_inner();
                        for attestation in attestations.attestations {
                            let state_map = height_to_state.entry(attestation.height).or_default();
                            state_map
                                .entry(State::from_slice(&attestation.state))
                                .or_default()
                                .push((Signature::from_slice(&attestation.signature), Pubkey::from_slice(&attestations.pubkey.clone())));
                        }
                    }
                    Err(e) => {
                        tracing::warn!("An attestor query failed: {}", e);
                    }
                }
            }
        }).await;
        
        if let Err(e) = collection_result {
            return Err(Status::internal(format!(
                "Attestor collection timed out after {:?}. Error: {:?}",
                attestor_query_timeout, e
            )));
        }
        
        let canidate = get_latest_state(&height_to_state, cached_height.clone(), self.config.quorum_threshold);
        if canidate.height < min_height {
            return Err(Status::not_found(format!(
                "No valid attestation found for height >= {}", min_height
            )));
        }
        drop(cached_height);

        let mut cached_height = self.cached_height.write().await;
        *cached_height = canidate.clone();
        Ok(Response::new(canidate))
    }
}

fn get_latest_state(
    height_to_state: &HashMap<Height, SignedStates>,
    cached_agg: AggregateResponse,
    quorum: usize,
) -> AggregateResponse {
    let mut agg = cached_agg;

    for (height, state_map) in height_to_state.iter() {
        // If we have more than one state at this height, raise some monitoring warning.
        if state_map.keys().len() > 1 {
            // TODO: Decide how to raise multiple states for a height.
            println!("multiple [{}] state found for height {}", state_map.keys().len(), height);
        }
        if *height <= agg.height {
            continue; // Skip heights lower than the cached height
        }
        
        for (state, sig_to_pks) in state_map.iter() {
            if sig_to_pks.len() < quorum {
                continue;
            }

            agg = AggregateResponse {
                height: *height,
                state: state.to_vec(),
                sig_pubkey_pairs: sig_to_pks
                    .iter()
                    .map(|(sig, pubkey)| SigPubkeyPair { sig: sig.to_vec(), pubkey: pubkey.to_vec() })
                    .collect(),
            };
        }
    }
    agg
}

#[cfg(test)]
mod e2e_tests {
    use super::*;
    use crate::{
        mock_attestor::MockAttestor,
        config::Config,
        rpc::attestor_server::{AttestorServer},
    };
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tonic::transport::Server;
    use url::Url;
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
    async fn get_aggregate_attestation_quorum_met() {
        let _ = tracing_subscriber::fmt::try_init();

        // 1. Setup: Create 3 successful attestors and 1 failing attestor.
        let (addr_1, pk_1) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_2, pk_2) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_3, pk_3) = setup_attestor_server(false, 0).await.unwrap();
        let (addr_4, _) = setup_attestor_server(true, 0).await.unwrap(); // This one will fail
        
        // 2. Setup: Create AggregatorService
        let config = Config {
            attestor_endpoints: vec![
                Url::parse(&format!("http://{}", addr_1)).unwrap(),
                Url::parse(&format!("http://{}", addr_2)).unwrap(),
                Url::parse(&format!("http://{}", addr_3)).unwrap(),
                Url::parse(&format!("http://{}", addr_4)).unwrap(),
            ],
            quorum_threshold: 3,
            listen_addr: "127.0.0.1:50060".parse().unwrap(), // Not used in this test
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
#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build a FixedBytes-N vector filled with `b`
    fn fill_bytes<const N: usize>(b: u8) -> Vec<u8> {
        vec![b; N]
    }

    #[test]
    fn returns_cached_if_no_states() {
        let height_to_state: HashMap<Height, SignedStates> = HashMap::new();
        let cached = AggregateResponse {
            height: 42,
            state: fill_bytes::<32>(0xAA),
            sig_pubkey_pairs: vec![],
        };
        let out = get_latest_state(&height_to_state, cached.clone(), 1);
        assert_eq!(out, cached);
    }

    #[test]
    fn ignores_states_below_quorum() {
        // We have a height 100 but only 1 signature < quorum 2
        let mut height_to_state = HashMap::new();
        let mut states = SignedStates::new();
        let st = State::from_slice(&fill_bytes::<32>(1));
        states.insert(
            st,
            vec![
                (Signature::from_slice(&fill_bytes::<32>(0x01)),
                 Pubkey::from_slice(&fill_bytes::<65>(0x02))),
            ],
        );
        height_to_state.insert(100, states);

        let cached = AggregateResponse {
            height: 50,
            state: fill_bytes::<32>(0xFF),
            sig_pubkey_pairs: vec![],
        };
        let out = get_latest_state(&height_to_state, cached.clone(), 2); // Quorum 2
        // Should still be the cached one
        assert_eq!(out, cached);
    }

    #[test]
    fn picks_single_height_meeting_quorum() {
        let mut height_to_state = HashMap::new();
        let mut states = SignedStates::new();
        let st = State::from_slice(&fill_bytes::<32>(7));
        // quorum = 2, so supply two signatures
        states.insert(
            st,
            vec![
                (Signature::from_slice(&fill_bytes::<32>(0x11)), Pubkey::from_slice(&fill_bytes::<65>(0x21))),
                (Signature::from_slice(&fill_bytes::<32>(0x12)), Pubkey::from_slice(&fill_bytes::<65>(0x22))),
            ],
        );
        height_to_state.insert(123, states);

        let cached = AggregateResponse {
            height: 100,
            state: fill_bytes::<32>(0x00),
            sig_pubkey_pairs: vec![],
        };
        let out = get_latest_state(&height_to_state, cached, 2); // Quorum 2

        assert_eq!(out.height, 123);
        assert_eq!(out.state, st.to_vec());
        // Should have two SigPubkeyPair entries
        assert_eq!(out.sig_pubkey_pairs.len(), 2);
        // Check that the pairs contain the pubkeys we inserted
        let pubs: Vec<_> = out.sig_pubkey_pairs.into_iter().map(|p| p.pubkey).collect();
        assert!(pubs.contains(&fill_bytes::<65>(0x21)));
        assert!(pubs.contains(&fill_bytes::<65>(0x22)));
    }

    #[test]
    fn chooses_highest_height_when_multiple() {
        let mut height_to_state = HashMap::new();

        // Height 200 meets quorum
        let mut s200 = SignedStates::new();
        let st200 = State::from_slice(&fill_bytes::<32>(0xAA));
        s200.insert(
            st200,
            vec![
                (Signature::from_slice(&fill_bytes::<32>(1)), Pubkey::from_slice(&fill_bytes::<65>(2))),
                (Signature::from_slice(&fill_bytes::<32>(3)), Pubkey::from_slice(&fill_bytes::<65>(4))),
                (Signature::from_slice(&fill_bytes::<32>(5)), Pubkey::from_slice(&fill_bytes::<65>(6))),
            ],
        );
        height_to_state.insert(200, s200);

        // Height 150 also meets quorum
        let mut s150 = SignedStates::new();
        let st150 = State::from_slice(&fill_bytes::<32>(0xBB));
        s150.insert(
            st150,
            vec![
                (Signature::from_slice(&fill_bytes::<32>(9)), Pubkey::from_slice(&fill_bytes::<65>(10))),
                (Signature::from_slice(&fill_bytes::<32>(11)), Pubkey::from_slice(&fill_bytes::<65>(12))),
                (Signature::from_slice(&fill_bytes::<32>(13)), Pubkey::from_slice(&fill_bytes::<65>(14))),
            ],
        );
        height_to_state.insert(150, s150);

        let cached = AggregateResponse {
            height: 100,
            state: fill_bytes::<32>(0x00),
            sig_pubkey_pairs: vec![],
        };
        let out = get_latest_state(&height_to_state, cached, /* quorum */ 3);

        // Should pick height 200, not 150
        assert_eq!(out.height, 200);
        assert_eq!(out.state, st200.to_vec());
    }
}
