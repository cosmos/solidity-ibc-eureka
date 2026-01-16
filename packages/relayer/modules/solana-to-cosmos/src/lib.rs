//! This is a one-sided relayer module from Solana to a Cosmos SDK chain.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(unused_crate_dependencies, missing_docs)]

pub mod tx_builder;

use std::collections::HashMap;

use ibc_eureka_relayer_lib::aggregator::{Aggregator, Config as AggregatorConfig};
use ibc_eureka_relayer_lib::events::EurekaEventWithHeight;
use ibc_eureka_relayer_lib::events::SolanaEurekaEventWithHeight;
use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_relayer_lib::listener::solana;
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

/// The `SolanaToCosmosRelayerModule` struct defines the Solana to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
pub struct SolanaToCosmosRelayerModule;

/// The `SolanaToCosmosRelayerModuleService` defines the relayer service from Solana to Cosmos.
struct SolanaToCosmosRelayerModuleService {
    /// The Solana chain ID for identification.
    solana_chain_id: String,
    /// The source chain listener for Solana.
    src_listener: solana::ChainListener,
    /// The target chain listener for Cosmos SDK.
    target_listener: cosmos_sdk::ChainListener,
    /// The transaction builder from Solana to Cosmos.
    tx_builder: SolanaToCosmosTxBuilder,
}

enum SolanaToCosmosTxBuilder {
    Mock(tx_builder::MockTxBuilder),
    Attested(tx_builder::AttestedTxBuilder),
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
    pub signer_address: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
    /// Transaction builder mode.
    #[serde(default)]
    pub mode: TxBuilderMode,
}

/// Transaction builder mode configuration.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum TxBuilderMode {
    /// Mock mode for testing without real proofs.
    #[default]
    Mock,
    /// Attested mode using aggregator attestations.
    Attested {
        /// Aggregator configuration.
        aggregator_config: AggregatorConfig,
    },
}

impl SolanaToCosmosRelayerModuleService {
    async fn new(config: SolanaToCosmosConfig) -> anyhow::Result<Self> {
        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana program ID: {e}"))?;

        let src_listener =
            solana::ChainListener::new(config.src_rpc_url.clone(), solana_ics26_program_id);

        let tm_client = HttpClient::from_rpc_url(&config.target_rpc_url);
        let target_listener = cosmos_sdk::ChainListener::new(tm_client.clone());

        let tx_builder = match config.mode {
            TxBuilderMode::Mock => SolanaToCosmosTxBuilder::Mock(tx_builder::MockTxBuilder::new(
                src_listener.client().clone(),
                target_listener.client().clone(),
                config.signer_address,
                solana_ics26_program_id,
            )),
            TxBuilderMode::Attested { aggregator_config } => {
                let aggregator = Aggregator::from_config(aggregator_config)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to create aggregator: {e}"))?;
                SolanaToCosmosTxBuilder::Attested(tx_builder::AttestedTxBuilder::new(
                    aggregator,
                    config.signer_address,
                ))
            }
        };

        Ok(Self {
            solana_chain_id: config.solana_chain_id,
            src_listener,
            target_listener,
            tx_builder,
        })
    }
}

#[tonic::async_trait]
impl RelayerService for SolanaToCosmosRelayerModuleService {
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
                chain_id: self.solana_chain_id.clone(),
                ibc_version: "2".to_string(),
                ibc_contract: self.src_listener.ics26_program_id().to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Solana to Cosmos...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        let solana_tx_hashes = parse_solana_tx_hashes(inner_req.source_tx_ids)?;
        let timeout_txs = parse_cosmos_tx_hashes(inner_req.timeout_tx_ids)?;

        let solana_events = self
            .src_listener
            .fetch_tx_events(solana_tx_hashes)
            .await
            .map_err(to_tonic_status)?;

        tracing::debug!(?solana_events, "Fetched source Solana events.");
        tracing::info!(
            "Fetched {} source eureka events from Solana.",
            solana_events.len()
        );

        let has_timeouts = !timeout_txs.is_empty();

        let timeout_events = self
            .target_listener
            .fetch_tx_events(timeout_txs)
            .await
            .map_err(to_tonic_status)?;

        tracing::debug!(?timeout_events, "Fetched timeout events from Cosmos.");
        tracing::info!(
            "Fetched {} timeout eureka events from CosmosSDK.",
            timeout_events.len()
        );

        // For timeouts, get the current slot from the source chain (Solana) where non-membership is proven
        let timeout_relay_height = if has_timeouts {
            Some(self.src_listener.get_slot().map_err(to_tonic_status)?)
        } else {
            None
        };

        let tx = self
            .tx_builder
            .relay_events(
                solana_events,
                timeout_events,
                timeout_relay_height,
                inner_req.src_client_id,
                inner_req.dst_client_id,
                inner_req.src_packet_sequences,
                inner_req.dst_packet_sequences,
            )
            .await
            .map_err(to_tonic_status)?;

        tracing::info!("Relay by tx request completed.");

        Ok(Response::new(api::RelayByTxResponse {
            tx,
            address: String::new(),
        }))
    }

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
            .map_err(to_tonic_status)?;

        Ok(Response::new(api::CreateClientResponse {
            tx,
            address: String::new(),
        }))
    }

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
            .map_err(to_tonic_status)?;

        tracing::info!("Update client request completed.");

        Ok(Response::new(api::UpdateClientResponse {
            tx,
            address: String::new(),
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
        let service = SolanaToCosmosRelayerModuleService::new(config).await?;
        Ok(Box::new(service))
    }
}

impl SolanaToCosmosTxBuilder {
    async fn relay_events(
        &self,
        src_events: Vec<SolanaEurekaEventWithHeight>,
        target_events: Vec<EurekaEventWithHeight>,
        timeout_relay_height: Option<u64>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> anyhow::Result<Vec<u8>> {
        match self {
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
            Self::Attested(tb) => {
                tb.relay_events(
                    src_events,
                    target_events,
                    timeout_relay_height,
                    &src_client_id,
                    &dst_client_id,
                    &src_packet_seqs,
                    &dst_packet_seqs,
                )
                .await
            }
        }
    }

    async fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Mock(tb) => tb.create_client(parameters).await,
            Self::Attested(tb) => tb.create_client(parameters),
        }
    }

    async fn update_client(&self, dst_client_id: String) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Mock(tb) => tb.update_client(dst_client_id).await,
            Self::Attested(tb) => tb.update_client(&dst_client_id).await,
        }
    }
}
