use std::collections::HashMap;
use tokio::sync::mpsc;
use tonic::{transport::Channel, Request, Response, Status};

use crate::{
    config::Config,
    error::AggregatorError,
    rpc::{
        aggregator_server::{Aggregator, AggregatorServer},
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
        // Drop the original sender to close the channel when all clones are dropped.
        drop(tx);

        let mut all_attestations = Vec::new();
        let mut responses_received = 0;
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
        tracing::info!(
            "Received responses from {} of {} attestors",
            responses_received,
            self.attestor_clients.len()
        );

        // Aggregate the results
        // HashMap<height, HashMap<signature, count>>
        let mut sig_counts: HashMap<u64, HashMap<Vec<u8>, usize>> = HashMap::new();
        for attestation in all_attestations {
            *sig_counts
                .entry(attestation.height)
                .or_default()
                .entry(attestation.signature)
                .or_default() += 1;
        }

        // Find the highest height with a quorum
        let best_attestation = sig_counts
            .into_iter()
            .flat_map(|(height, sig_map)| {
                sig_map.into_iter().filter_map(move |(signature, count)| {
                    if count >= self.config.quorum_threshold {
                        Some(Attestation { height, signature })
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
