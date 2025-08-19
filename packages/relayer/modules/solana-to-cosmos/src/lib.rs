//! This is a one-sided relayer module from Solana to a Cosmos SDK chain.
//!
//! Note: This module does not use SP1 proofs since Solana uses Proof of History,
//! not Tendermint consensus. Instead, it would use a WASM light client on Cosmos
//! to verify Solana's `PoH` consensus.

#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]
#![allow(unused_crate_dependencies)]

pub mod tx_builder;

use std::collections::HashMap;
use std::sync::Arc;

use ibc_eureka_utils::rpc::TendermintRpcExt;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use tendermint::Hash;
use tendermint_rpc::Client;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

/// The `SolanaToCosmosRelayerModule` struct defines the Solana to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
pub struct SolanaToCosmosRelayerModule;

/// The `SolanaToCosmosRelayerModuleService` defines the relayer service from Solana to Cosmos.
#[allow(dead_code)]
struct SolanaToCosmosRelayerModuleService {
    /// The Solana RPC client (wrapped in Arc since `RpcClient` doesn't implement Clone in 2.0).
    pub solana_client: Arc<RpcClient>,
    /// The target Cosmos tendermint client.
    pub target_tm_client: HttpClient,
    /// The transaction builder from Solana to Cosmos.
    pub tx_builder: tx_builder::TxBuilder,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: solana_sdk::pubkey::Pubkey,
}

/// The configuration for the Solana to Cosmos relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SolanaToCosmosConfig {
    /// The Solana RPC URL.
    pub solana_rpc_url: String,
    /// The target tendermint RPC URL.
    pub target_rpc_url: String,
    /// The address of the submitter on Cosmos.
    /// Required since cosmos messages require a signer address.
    pub signer_address: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
}

impl SolanaToCosmosRelayerModuleService {
    fn new(config: SolanaToCosmosConfig) -> anyhow::Result<Self> {
        let solana_client = Arc::new(RpcClient::new(config.solana_rpc_url));
        let target_client = HttpClient::from_rpc_url(&config.target_rpc_url);

        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana program ID: {}", e))?;

        let tx_builder = tx_builder::TxBuilder::new(
            Arc::clone(&solana_client),
            target_client.clone(),
            config.signer_address,
            solana_ics26_program_id,
        );

        Ok(Self {
            solana_client,
            target_tm_client: target_client,
            tx_builder,
            solana_ics26_program_id,
        })
    }
}

#[tonic::async_trait]
impl RelayerService for SolanaToCosmosRelayerModuleService {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Solana to Cosmos...");

        // Get Cosmos chain ID
        let status = self
            .target_tm_client
            .status()
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to get chain ID: {e}")))?;

        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: status.node_info.network.to_string(),
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            source_chain: Some(api::Chain {
                chain_id: "solana".to_string(), // Solana doesn't have chain IDs like Cosmos
                ibc_version: "2".to_string(),
                ibc_contract: self.solana_ics26_program_id.to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Solana to Cosmos...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        // Parse Solana transaction signatures
        let src_txs: Vec<Signature> = inner_req
            .source_tx_ids
            .into_iter()
            .map(|tx_id| {
                let sig_str = String::from_utf8(tx_id).map_err(|e| {
                    tonic::Status::invalid_argument(format!("Invalid signature: {e}"))
                })?;
                sig_str
                    .parse::<Signature>()
                    .map_err(|e| tonic::Status::invalid_argument(format!("Invalid signature: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Parse Cosmos transaction hashes for timeouts
        let _target_txs = inner_req
            .timeout_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        // Fetch events from Solana transactions
        let src_events = self
            .tx_builder
            .fetch_solana_events(src_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(solana_src_events = ?src_events, "Fetched source Solana events.");
        tracing::info!(
            "Fetched {} source eureka events from Solana.",
            src_events.len()
        );

        // For timeouts from Cosmos - simplified for now
        let target_events = Vec::new();
        tracing::info!(
            "Fetched {} target eureka events from CosmosSDK.",
            target_events.len()
        );

        // Build the relay transaction
        let tx = self
            .tx_builder
            .build_relay_tx(src_events, target_events)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!(
            "Built {} messages for Solana to Cosmos relay.",
            tx.messages.len()
        );

        Ok(Response::new(api::RelayByTxResponse {
            tx: serde_json::to_vec(&tx).map_err(|e| tonic::Status::from_error(e.into()))?,
            address: String::new(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Solana to Cosmos...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .build_create_client_tx(inner_req.parameters)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        Ok(Response::new(api::CreateClientResponse {
            tx: serde_json::to_vec(&tx).map_err(|e| tonic::Status::from_error(e.into()))?,
            address: String::new(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Solana to Cosmos...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .build_update_client_tx(inner_req.dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        Ok(Response::new(api::UpdateClientResponse {
            tx: serde_json::to_vec(&tx).map_err(|e| tonic::Status::from_error(e.into()))?,
            address: String::new(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for SolanaToCosmosRelayerModule {
    fn name(&self) -> &'static str {
        "solana-to-cosmos"
    }

    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config: SolanaToCosmosConfig = serde_json::from_value(config)?;
        let service = SolanaToCosmosRelayerModuleService::new(config)?;
        Ok(Box::new(service))
    }
}
