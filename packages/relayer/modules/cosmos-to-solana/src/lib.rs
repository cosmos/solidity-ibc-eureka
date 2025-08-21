//! This is a one-sided relayer module from a Cosmos SDK chain to Solana.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(missing_docs, unused_crate_dependencies)]

pub mod tx_builder;

use std::collections::HashMap;
use std::sync::Arc;

use ibc_eureka_utils::rpc::TendermintRpcExt;
use solana_client::rpc_client::RpcClient;
use tendermint::Hash;
use tendermint_rpc::Client;
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
    /// The source Cosmos tendermint client.
    pub source_tm_client: HttpClient,
    /// The target Solana RPC client
    pub solana_client: Arc<RpcClient>,
    /// The transaction builder from Cosmos to Solana.
    pub tx_builder: tx_builder::TxBuilder,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: solana_sdk::pubkey::Pubkey,
    /// The Solana ICS07 Tendermint light client program ID.
    pub solana_ics07_program_id: solana_sdk::pubkey::Pubkey,
}

/// The configuration for the Cosmos to Solana relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CosmosToSolanaConfig {
    /// The source tendermint RPC URL.
    pub source_rpc_url: String,
    /// The Solana RPC URL.
    pub solana_rpc_url: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
    /// The Solana ICS07 Tendermint light client program ID.
    pub solana_ics07_program_id: String,
    /// The Solana wallet keypair path (for signing transactions).
    pub solana_wallet_path: String,
}

impl CosmosToSolanaRelayerModuleService {
    fn new(config: &CosmosToSolanaConfig) -> anyhow::Result<Self> {
        let source_client = HttpClient::from_rpc_url(&config.source_rpc_url);
        let solana_client = Arc::new(RpcClient::new(config.solana_rpc_url.clone()));

        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS26 program ID: {}", e))?;

        let solana_ics07_program_id = config
            .solana_ics07_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS07 program ID: {}", e))?;

        let tx_builder = tx_builder::TxBuilder::new(
            source_client.clone(),
            solana_client.clone(),
            solana_ics26_program_id,
            solana_ics07_program_id,
            &config.solana_wallet_path,
        )?;

        Ok(Self {
            source_tm_client: source_client,
            solana_client,
            tx_builder,
            solana_ics26_program_id,
            solana_ics07_program_id,
        })
    }
}

#[tonic::async_trait]
impl RelayerService for CosmosToSolanaRelayerModuleService {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Cosmos to Solana...");

        // Get Cosmos chain ID
        let status = self
            .source_tm_client
            .status()
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to get chain ID: {e}")))?;

        Ok(Response::new(api::InfoResponse {
            source_chain: Some(api::Chain {
                chain_id: status.node_info.network.to_string(),
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            target_chain: Some(api::Chain {
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
        tracing::info!("Handling relay by tx request for Cosmos to Solana...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        // Parse Cosmos transaction hashes
        let src_txs = inner_req
            .source_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        // Parse Solana transaction signatures for timeouts
        let target_txs: Vec<solana_sdk::signature::Signature> = inner_req
            .timeout_tx_ids
            .into_iter()
            .map(|tx_id| {
                let sig_str = String::from_utf8(tx_id).map_err(|e| {
                    tonic::Status::invalid_argument(format!("Invalid signature: {e}"))
                })?;
                sig_str
                    .parse::<solana_sdk::signature::Signature>()
                    .map_err(|e| tonic::Status::invalid_argument(format!("Invalid signature: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Fetch events from Cosmos transactions
        let src_events = self
            .tx_builder
            .fetch_cosmos_events(src_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(cosmos_src_events = ?src_events, "Fetched source Cosmos events.");
        tracing::info!(
            "Fetched {} source eureka events from Cosmos.",
            src_events.len()
        );

        // Fetch events from Solana for timeouts
        let target_events = self
            .tx_builder
            .fetch_solana_timeout_events(target_txs)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(solana_target_events = ?target_events, "Fetched target Solana events.");
        tracing::info!(
            "Fetched {} target eureka events from Solana.",
            target_events.len()
        );

        let tx = self
            .tx_builder
            .build_solana_tx(src_events, target_events)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Built Solana transaction for Cosmos to Solana relay.");

        let tx_bytes = bincode::serialize(&tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize transaction: {e}"))
        })?;

        Ok(Response::new(api::RelayByTxResponse {
            tx: tx_bytes,
            address: self.solana_ics26_program_id.to_string(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Cosmos to Solana...");

        let inner_req = request.into_inner();

        let tx = self
            .tx_builder
            .build_create_client_tx(inner_req.parameters)
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

        let tx = self
            .tx_builder
            .build_update_client_tx(inner_req.dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        let tx_bytes = bincode::serialize(&tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize transaction: {e}"))
        })?;

        Ok(Response::new(api::UpdateClientResponse {
            tx: tx_bytes,
            address: self.solana_ics07_program_id.to_string(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for CosmosToSolanaRelayerModule {
    fn name(&self) -> &'static str {
        "cosmos-to-solana"
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
