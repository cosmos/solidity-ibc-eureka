use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tonic::{transport::Channel, Request, Response, Status};

use crate::{
    config::Config,
    error::AggregatorError,
    rpc::{
        aggregator_server::{Aggregator},
        attestor_client::AttestorClient,
        AggregateRequest, AggregateResponse, Attestation, QueryRequest,
    },
};

#[derive(Debug)]
pub struct AggregatorService {
    config: Config,
    attestor_clients: Vec<AttestorClient<Channel>>,
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
        
        let mut all_attestations = Vec::new();
        let mut responses_received = 0;
        let attestor_query_timeout = Duration::from_millis(self.config.attestor_query_timeout_ms);
        let collection_result = timeout(attestor_query_timeout, async {
            while let Some(result) = rx.recv().await {
                responses_received += 1;
                match result {
                    Ok(response) => {
                        all_attestations.extend(response.into_inner().attestations);
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
        
        // HashMap<height, HashMap<(signature, pubKey), count>>
        let mut sig_counts: HashMap<u64, HashMap<Vec<u8>, (usize, Vec<u8>)>> = HashMap::new();
        // let mut sig_to_pubkey: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
        for attestation in all_attestations {
            let inner_map = sig_counts.entry(attestation.height).or_default();
            let entry = inner_map.entry(attestation.signature.clone());
            match entry {
                std::collections::hash_map::Entry::Occupied(mut o) => {
                    o.get_mut().0 += 1;
                }
                std::collections::hash_map::Entry::Vacant(v) => {
                    v.insert((1, attestation.pubkey.clone()));
                }
            }
        }

        // Find the highest height with a quorum
        let best_attestation = sig_counts
            .into_iter()
            .flat_map(|(height, sig_map)| {
                sig_map.into_iter().filter_map({
                move |(signature, (count, pubkey))| {
                    if count >= self.config.quorum_threshold {
                        Some(Attestation { 
                            height, 
                            pubkey,
                            signature,
                        })
                    } else {
                        None
                    }
                }
                })
            })
            .max_by_key(|a| a.height);
        
        if let Some(attestation) = &best_attestation {
            tracing::info!(
                "Found quorum for height {}: signature 0x{}",
                attestation.height,
                hex::encode(&attestation.signature)
            );
        } else {
            tracing::warn!("No quorum found for any height >= {}", min_height);
        }

        Ok(Response::new(AggregateResponse {
            attestation: best_attestation,
        }))
    }
}

// let (sk, pk) = generate_keypair(&mut rand::rng());
// let msg = Message::from_digest([0; 32]);

// let mut att = AttestationData{
//     data: ChainHeader{
//         chain_id: 0,
//         height: 0,
//         state_root: Vec::new(),
//         timestamp: 100,
//     },
//     signature: sk.sign_ecdsa(msg),
//     pubkey: pk,
// };

// att.signature.serialize_compact(); // u8; 64
// att.pubkey.serialize_uncompressed(); // u8; 65

/*



use secp256k1::PublicKey;
use secp256k1::ecdsa::Signature;

#[derive(Debug)]
pub struct MultiSigAttestation {
    pub attestation_data: AttestationData,
    pub pubkeys: Vec<PublicKey>,
    pub signatures: Vec<Signature>,
    }
    
    
*/

// #[derive(Debug)]
// pub struct ChainHeader {
//     pub chain_id: u64,
//     pub height: u64,
//     pub state_root: Vec<u8>,
//     pub timestamp: u64,
// }

// #[derive(Debug)]
// pub struct AttestationData {
//     pub data: ChainHeader,
//     pub signature: Signature,
//     pub pubkey: PublicKey,
// }

// /// A multi-signature attestation, collecting N individual attestations on the same data.
// /// Ensures all attestations refer to identical state and preserves public keys and signatures in order.
// #[derive(Debug)]
// pub struct MultiSigAttestation {
//     pub chain_header: ChainHeader,
//     pub pubkeys: Vec<PublicKey>,
//     pub signatures: Vec<Signature>,
// }

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

    // Helper to spin up a mock attestor server on a random available port.
    // Returns the address it's listening on.
    async fn setup_attestor_server(should_fail: bool, delay_ms: u64) -> anyhow::Result<SocketAddr> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let attestor = MockAttestor::new(should_fail, delay_ms);

        tokio::spawn(async move {
            Server::builder()
                .add_service(AttestorServer::new(attestor))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
        });

        Ok(addr)
    }

    fn get_mock_signature(height: u64) -> Vec<u8> {
        let mut sig = [0u8; 32];
        let height_bytes = height.to_be_bytes();
        for i in 0..4 {
            sig[i * 8..(i + 1) * 8].copy_from_slice(&height_bytes);
        }
        sig.to_vec()
    }

    #[tokio::test]
    async fn test_get_aggregate_attestation_quorum_met() {
        // 1. Setup: Create 3 successful attestors and 1 failing attestor.
        let attestor_addr_1 = setup_attestor_server(false, 0).await.unwrap();
        let attestor_addr_2 = setup_attestor_server(false, 0).await.unwrap();
        let attestor_addr_3 = setup_attestor_server(false, 0).await.unwrap();
        let attestor_addr_4 = setup_attestor_server(true, 0).await.unwrap(); // This one will fail

        // 2. Setup: Create AggregatorService
        let config = Config {
            attestor_endpoints: vec![
                format!("http://{}", attestor_addr_1),
                format!("http://{}", attestor_addr_2),
                format!("http://{}", attestor_addr_3),
                format!("http://{}", attestor_addr_4),
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
        let attestation = response.into_inner().attestation.unwrap();

        // From `MockAttestor::new`, the highest height all 3 successful attestors agree on is 110.
        assert_eq!(attestation.height, 110);
        assert_eq!(attestation.signature, get_mock_signature(110));
        assert_eq!(attestation.pubkey.len(), 65);
        assert!(attestation.pubkey.iter().any(|&b| b != 0));
    }
}