//! This is a one-sided relayer module from a Cosmos SDK chain to Solana.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(missing_docs, unused_crate_dependencies)]

pub mod tx_builder;

use std::collections::HashMap;

use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_relayer_lib::listener::solana_eureka;
use ibc_eureka_relayer_lib::listener::ChainListenerService;
use ibc_eureka_relayer_lib::service_utils::parse_cosmos_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::parse_solana_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::to_tonic_status;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use solana_sdk::pubkey::Pubkey;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

/// The `CosmosToSolanaRelayerModule` struct defines the Cosmos to Solana relayer module.
#[derive(Clone, Copy, Debug)]
pub struct CosmosToSolanaRelayerModule;

/// The `CosmosToSolanaRelayerModuleService` defines the relayer service from Cosmos to Solana.
#[allow(dead_code)]
struct CosmosToSolanaRelayerModuleService {
    /// The souce chain listener for Cosmos.
    pub src_listener: cosmos_sdk::ChainListener,
    /// The target chain listener for Solana.
    pub target_listener: solana_eureka::ChainListener,
    /// The transaction builder from Cosmos to Solana.
    pub tx_builder: tx_builder::TxBuilder,
    /// The Solana ICS07 program ID.
    pub solana_ics07_program_id: Pubkey,
}

/// The configuration for the Cosmos to Solana relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CosmosToSolanaConfig {
    /// The source tendermint RPC URL.
    pub source_rpc_url: String,
    /// The Solana RPC URL.
    pub target_rpc_url: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
    /// The Solana ICS07 Tendermint light client program ID.
    pub solana_ics07_program_id: String,
    /// The Solana fee payer address.
    pub solana_fee_payer: String,
}

impl CosmosToSolanaRelayerModuleService {
    fn new(config: &CosmosToSolanaConfig) -> anyhow::Result<Self> {
        let src_listener =
            cosmos_sdk::ChainListener::new(HttpClient::from_rpc_url(&config.source_rpc_url));

        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS26 program ID: {}", e))?;

        let solana_ics07_program_id = config
            .solana_ics07_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS07 program ID: {}", e))?;

        let target_listener = solana_eureka::ChainListener::new(
            config.target_rpc_url.clone(),
            solana_ics26_program_id,
        );

        let fee_payer = config
            .solana_fee_payer
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid fee payer address: {}", e))?;

        let tx_builder = tx_builder::TxBuilder::new(
            src_listener.client().clone(),
            target_listener.client().clone(),
            solana_ics07_program_id.clone(),
            solana_ics26_program_id.clone(),
            fee_payer,
        )?;

        Ok(Self {
            src_listener,
            target_listener,
            tx_builder,
            solana_ics07_program_id,
        })
    }
}

#[async_trait::async_trait]
impl RelayerService for CosmosToSolanaRelayerModuleService {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Cosmos to Solana...");

        Ok(Response::new(api::InfoResponse {
            source_chain: Some(api::Chain {
                chain_id: self
                    .src_listener
                    .chain_id()
                    .await
                    .map_err(to_tonic_status)?,
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            target_chain: Some(api::Chain {
                chain_id: "solana".to_string(), // Solana doesn't have chain IDs like Cosmos
                ibc_version: "2".to_string(),
                ibc_contract: self.target_listener.ics26_program_id().to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

    // NOTE: Client would not be automatically updated and should be done manually
    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Cosmos to Solana...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());
        let src_txs = parse_cosmos_tx_hashes(inner_req.source_tx_ids)?;

        let target_txs = parse_solana_tx_hashes(inner_req.timeout_tx_ids)?;

        let src_events = self
            .src_listener
            .fetch_tx_events(src_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(cosmos_src_events = ?src_events, "Fetched source cosmos events.");
        tracing::info!(
            "Fetched {} source eureka events from CosmosSDK.",
            src_events.len()
        );

        let target_events = self
            .target_listener
            .fetch_tx_events(target_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(solana_target_events = ?target_events, "Fetched target Solana events.");
        tracing::info!(
            "Fetched {} target eureka events from Solana.",
            target_events.len()
        );

        // TODO: get tx
        unimplemented!()
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        _request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Cosmos to Solana...");

        let tx = self
            .tx_builder
            .build_create_client_tx()
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        let tx_bytes = bincode::serialize(&tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize transaction: {e}"))
        })?;

        Ok(Response::new(api::CreateClientResponse {
            tx: tx_bytes,
            address: self.solana_ics07_program_id.to_string(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Cosmos to Solana...");

        let inner_req = request.into_inner();
        let client_id = inner_req.dst_client_id;

        let chunked_txs = self
            .tx_builder
            .build_chunked_update_client_params(client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!(
            "Built {} transactions for chunked update client (1 metadata + {} chunks + 1 assembly)",
            chunked_txs.total_chunks + 2, // metadata + chunks + assembly
            chunked_txs.total_chunks
        );

        // Serialize all transactions for the chunked_txs field
        let mut serialized_txs = Vec::new();

        // Add metadata
        serialized_txs.push(bincode::serialize(&chunked_txs.metadata_tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize metadata tx: {e}"))
        })?);

        // Add all chunk transactions (can be sent in parallel after metadata)
        for tx in &chunked_txs.chunk_txs {
            serialized_txs.push(bincode::serialize(tx).map_err(|e| {
                tonic::Status::internal(format!("Failed to serialize chunk tx: {e}"))
            })?);
        }

        // Add assembly transaction (must be last)
        serialized_txs.push(bincode::serialize(&chunked_txs.assembly_tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize assembly tx: {e}"))
        })?);

        Ok(Response::new(api::UpdateClientResponse {
            tx: vec![],
            address: self.solana_ics07_program_id.to_string(),
            txs: serialized_txs,
        }))
    }
}

struct ChunkedUpdateClient {
    pub header_chunks: Vec<u8>,
}

#[tonic::async_trait]
impl RelayerModule for CosmosToSolanaRelayerModule {
    fn name(&self) -> &'static str {
        "cosmos_to_solana"
    }

    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config: CosmosToSolanaConfig = serde_json::from_value(config)?;
        let service = CosmosToSolanaRelayerModuleService::new(&config)?;
        Ok(Box::new(service))
    }
}
