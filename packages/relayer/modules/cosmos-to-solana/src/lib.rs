//! This is a one-sided relayer module from a Cosmos SDK chain to Solana.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(missing_docs, unused_crate_dependencies)]

pub mod borsh_conversions;
pub mod constants;
pub mod gmp;
pub mod proto;
pub mod tx_builder;

use std::collections::HashMap;

use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_relayer_lib::listener::solana;
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
    pub target_listener: solana::ChainListener,
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
    /// Address Lookup Table address for reducing transaction size (optional).
    pub solana_alt_address: Option<String>,
    /// Whether to use mock WASM client on Cosmos for testing.
    pub mock_wasm_client: bool,
    /// Signature threshold below which pre-verification is skipped.
    /// None = always use pre-verification, Some(n) = skip when signatures ≤ n.
    /// Default: Some(50)
    #[serde(default = "default_skip_pre_verify_threshold")]
    pub skip_pre_verify_threshold: Option<usize>,
}

const fn default_skip_pre_verify_threshold() -> Option<usize> {
    Some(50)
}

impl CosmosToSolanaRelayerModuleService {
    fn new(config: &CosmosToSolanaConfig) -> anyhow::Result<Self> {
        let src_listener =
            cosmos_sdk::ChainListener::new(HttpClient::from_rpc_url(&config.source_rpc_url));

        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS26 program ID: {e}"))?;

        let solana_ics07_program_id: Pubkey = config
            .solana_ics07_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS07 program ID: {e}"))?;

        let target_listener =
            solana::ChainListener::new(config.target_rpc_url.clone(), solana_ics26_program_id);

        let fee_payer = config
            .solana_fee_payer
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid fee payer address: {e}"))?;

        let alt_address = config
            .solana_alt_address
            .as_ref()
            .map(|addr| addr.parse())
            .transpose()
            .map_err(|e| anyhow::anyhow!("Invalid ALT address: {e}"))?;

        let tx_builder = tx_builder::TxBuilder::new(
            src_listener.client().clone(),
            target_listener.client().clone(),
            solana_ics07_program_id,
            solana_ics26_program_id,
            fee_payer,
            alt_address,
            config.skip_pre_verify_threshold,
        )?;

        Ok(Self {
            src_listener,
            target_listener,
            tx_builder,
            solana_ics07_program_id,
        })
    }
}

#[tonic::async_trait]
impl RelayerService for CosmosToSolanaRelayerModuleService {
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
                chain_id: "solana-localnet".to_string(), // Solana doesn't have chain IDs like Cosmos
                ibc_version: "2".to_string(),
                ibc_contract: self.target_listener.ics26_program_id().to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

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

        // Use the combined method that includes update client if needed
        let (packet_txs, update_client) = self
            .tx_builder
            .relay_events_with_update(tx_builder::RelayParams {
                src_events,
                dest_events: target_events,
                src_client_id: inner_req.src_client_id,
                dst_client_id: inner_req.dst_client_id,
                src_packet_seqs: inner_req.src_packet_sequences,
                dst_packet_seqs: inner_req.dst_packet_sequences,
            })
            .await
            .map_err(to_tonic_status)?;

        tracing::info!(
            "Relay by tx request completed with {} packets{}.",
            packet_txs.len(),
            if update_client.is_some() {
                " (with update client)"
            } else {
                ""
            }
        );

        let batch = api::SolanaRelayPacketBatch {
            packets: packet_txs,
            update_client,
        };
        let tx = prost::Message::encode_to_vec(&batch);

        Ok(Response::new(api::RelayByTxResponse {
            tx,
            address: String::new(),
        }))
    }

    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Cosmos to Solana...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .create_client(&inner_req.parameters)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        Ok(Response::new(api::CreateClientResponse {
            tx,
            address: self.solana_ics07_program_id.to_string(),
        }))
    }

    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Cosmos to Solana...");

        let solana_update_client = self
            .tx_builder
            .update_client(request.into_inner().dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!(
            "Using chunked update client with {} signatures/chunks",
            solana_update_client.chunk_txs.len()
        );

        let tx = prost::Message::encode_to_vec(&solana_update_client);

        Ok(Response::new(api::UpdateClientResponse {
            tx,
            address: self.solana_ics07_program_id.to_string(),
        }))
    }
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
