//! This is a one-sided relayer module from Solana to a Cosmos SDK chain.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(unused_crate_dependencies, missing_docs)]

pub mod tx_builder;

use std::collections::HashMap;

use ibc_eureka_relayer_lib::events::EurekaEventWithHeight;
use ibc_eureka_relayer_lib::events::SolanaEurekaEventWithHeight;
use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_relayer_lib::listener::solana_eureka;
use ibc_eureka_relayer_lib::listener::ChainListenerService;
use ibc_eureka_relayer_lib::service_utils::parse_cosmos_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::parse_solana_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::to_tonic_status;
use ibc_eureka_relayer_lib::tx_builder::TxBuilderService;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

#[allow(dead_code)]
enum SolanaToCosmosTxBuilder {
    Real(),
    Mock(tx_builder::MockTxBuilder),
}

/// The `SolanaToCosmosRelayerModule` struct defines the Solana to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
pub struct SolanaToCosmosRelayerModule;

/// The `SolanaToCosmosRelayerModuleService` defines the relayer service from Solana to Cosmos.
#[allow(dead_code)]
struct SolanaToCosmosRelayerModuleService {
    /// The souce chain listener for Solana.
    pub src_listener: solana_eureka::ChainListener,
    /// The target chain listener for Cosmos SDK.
    pub target_listener: cosmos_sdk::ChainListener,
    /// The transaction builder from Solana to Cosmos.
    pub tx_builder: SolanaToCosmosTxBuilder,
}

/// The configuration for the Solana to Cosmos relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SolanaToCosmosConfig {
    /// The Solana chain ID for identification.
    pub solana_chain_id: String,
    /// The Solana RPC URL.
    pub src_rpc_url: String,
    /// The target tendermint RPC URL.
    pub target_rpc_url: String,
    /// The address of the submitter on Cosmos.
    /// Required since cosmos messages require a signer address.
    pub signer_address: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
    /// Whether to run in mock mode.
    #[serde(default)]
    pub mock: bool,
}

impl SolanaToCosmosRelayerModuleService {
    fn new(config: SolanaToCosmosConfig) -> anyhow::Result<Self> {
        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana program ID: {e}"))?;

        let src_listener =
            solana_eureka::ChainListener::new(config.src_rpc_url.clone(), solana_ics26_program_id);

        let target_listener =
            cosmos_sdk::ChainListener::new(HttpClient::from_rpc_url(&config.target_rpc_url));

        let tx_builder = if config.mock {
            SolanaToCosmosTxBuilder::Mock(tx_builder::MockTxBuilder::new(
                src_listener.client().clone(),
                target_listener.client().clone(),
                config.signer_address,
                solana_ics26_program_id,
            ))
        } else {
            // TODO: Implement once solana client for cosmos is ready
            unimplemented!()
        };

        Ok(Self {
            src_listener,
            target_listener,
            tx_builder,
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

        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: self
                    .target_listener
                    .chain_id()
                    .await
                    .map_err(to_tonic_status)?,
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            source_chain: Some(api::Chain {
                chain_id: "solana".to_string(), // Solana doesn't have chain IDs like Cosmos
                ibc_version: "2".to_string(),
                ibc_contract: self.src_listener.ics26_program_id().to_string(),
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

        let solana_tx_hashes = parse_solana_tx_hashes(inner_req.source_tx_ids)?;

        let cosmos_txs = parse_cosmos_tx_hashes(inner_req.timeout_tx_ids)?;

        let solana_events = self
            .src_listener
            .fetch_tx_events(solana_tx_hashes)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(?solana_events, "Fetched source Solana events.");
        tracing::info!(
            "Fetched {} source eureka events from Solana.",
            solana_events.len()
        );

        let cosmos_events = self
            .target_listener
            .fetch_tx_events(cosmos_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(cosmos_events = ?cosmos_events, "Fetched Cosmos events.");
        tracing::info!(
            "Fetched {} eureka events from CosmosSDK.",
            cosmos_events.len()
        );

        let tx = self
            .tx_builder
            .relay_events(
                solana_events,
                cosmos_events,
                inner_req.src_client_id,
                inner_req.dst_client_id,
                inner_req.src_packet_sequences,
                inner_req.dst_packet_sequences,
            )
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Relay by tx request completed.");

        Ok(Response::new(api::RelayByTxResponse {
            tx,
            address: String::new(),
            txs: vec![],
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
            .create_client(&inner_req.parameters)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        Ok(Response::new(api::CreateClientResponse {
            tx,
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
            .update_client(inner_req.dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Update client request completed.");

        Ok(Response::new(api::UpdateClientResponse {
            tx,
            address: String::new(),
            txs: vec![],
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for SolanaToCosmosRelayerModule {
    fn name(&self) -> &'static str {
        "solana_to_cosmos"
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

impl SolanaToCosmosTxBuilder {
    async fn relay_events(
        &self,
        src_events: Vec<SolanaEurekaEventWithHeight>,
        target_events: Vec<EurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Real() => unreachable!(),
            Self::Mock(tb) => {
                tb.relay_events(
                    src_events,
                    target_events,
                    src_client_id,
                    dst_client_id,
                    src_packet_seqs,
                    dst_packet_seqs,
                )
                .await
            }
        }
    }

    async fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Real() => unreachable!(),
            Self::Mock(tb) => tb.create_client(parameters).await,
        }
    }

    async fn update_client(&self, dst_client_id: String) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Real() => unreachable!(),
            Self::Mock(tb) => tb.update_client(dst_client_id).await,
        }
    }
}
