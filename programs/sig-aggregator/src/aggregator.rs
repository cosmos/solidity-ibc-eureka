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
        const ATTESTOR_QUERY_TIMEOUT: Duration = Duration::from_secs(5);

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
        let collection_result = timeout(ATTESTOR_QUERY_TIMEOUT, async {
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
            tracing::warn!("Attestor collection timed out after {:?}. Error: {:?}", ATTESTOR_QUERY_TIMEOUT, e);
        }
        
        // HashMap<height, HashMap<(signature, pubKey), count>>
        let mut sig_counts: HashMap<u64, HashMap<(Vec<u8>, Vec<u8>), usize>> = HashMap::new();
        for attestation in all_attestations {
            *sig_counts
                .entry(attestation.height)
                .or_default()
                .entry((attestation.signature, attestation.pubkey))
                .or_default() += 1;
        }

        // Find the highest height with a quorum
        let best_attestation = sig_counts
            .into_iter()
            .flat_map(|(height, sig_map)| {
                sig_map.into_iter().filter_map(move |(data, count)| {
                    if count >= self.config.quorum_threshold {
                        Some(Attestation { 
                            height, 
                            signature: data.0,
                            pubkey: data.1,
                        })
                    } else {
                        None
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
