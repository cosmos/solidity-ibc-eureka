//! This is a one-sided relayer module from Solana to a Cosmos SDK chain.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(unused_crate_dependencies, missing_docs)]

pub mod tx_builder;

use std::collections::HashMap;

use ibc_eureka_relayer_lib::events::{EurekaEventWithHeight, SolanaEurekaEventWithHeight};
use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_relayer_lib::listener::solana;
use ibc_eureka_relayer_lib::listener::ChainListenerService;
use ibc_eureka_relayer_lib::service_utils::parse_cosmos_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::parse_solana_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::to_tonic_status;
use ibc_eureka_relayer_lib::tx_builder::TxBuilderService;
use ibc_eureka_relayer_lib::utils::RelayEventsParams;
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
    Attested(tx_builder::AttestedTxBuilder),
}

/// The `SolanaToCosmosRelayerModule` struct defines the Solana to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
pub struct SolanaToCosmosRelayerModule;

/// The `SolanaToCosmosRelayerModuleService` defines the relayer service from Solana to Cosmos.
#[allow(dead_code)]
struct SolanaToCosmosRelayerModuleService {
    /// The souce chain listener for Solana.
    pub src_listener: solana::ChainListener,
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
    /// Whether to use mock WASM client on Cosmos for testing.
    #[serde(default)]
    pub mock_wasm_client: bool,
    /// Whether to use mock Solana light client updates for testing.
    #[serde(default)]
    pub mock_solana_client: bool,
}

impl SolanaToCosmosRelayerModuleService {
    fn new(config: SolanaToCosmosConfig) -> anyhow::Result<Self> {
        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana program ID: {e}"))?;

        let src_listener =
            solana::ChainListener::new(config.src_rpc_url.clone(), solana_ics26_program_id);

        let target_listener =
            cosmos_sdk::ChainListener::new(HttpClient::from_rpc_url(&config.target_rpc_url));

        let tx_builder = if config.mock_wasm_client {
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
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::debug!("Handling info request");

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
                chain_id: "solana-localnet".to_string(), // Solana doesn't have chain IDs like Cosmos
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
        let inner_req = request.into_inner();
        tracing::debug!(
            "Relay request: {} source txs, {} timeout txs",
            inner_req.source_tx_ids.len(),
            inner_req.timeout_tx_ids.len()
        );

        let solana_tx_hashes = parse_solana_tx_hashes(inner_req.source_tx_ids)?;
        let timeout_txs = parse_cosmos_tx_hashes(inner_req.timeout_tx_ids)?;

        let solana_events = self
            .src_listener
            .fetch_tx_events(solana_tx_hashes)
            .await
            .map_err(to_tonic_status)?;

        tracing::debug!("Fetched {} src events", solana_events.len());

        let timeout_events = self
            .target_listener
            .fetch_tx_events(timeout_txs)
            .await
            .map_err(to_tonic_status)?;

        tracing::debug!("Fetched {} timeout events", timeout_events.len());

        // For timeouts in attested mode, get the current slot from the source chain (Solana)
        // where non-membership is proven
        let timeout_relay_height = if self.tx_builder.is_attested() && !timeout_events.is_empty() {
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
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!("Relay completed");

        Ok(Response::new(api::RelayByTxResponse {
            tx,
            address: String::new(),
        }))
    }

    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::debug!("Handling create client request");

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

    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .update_client(inner_req.dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

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
        let service = SolanaToCosmosRelayerModuleService::new(config)?;
        Ok(Box::new(service))
    }
}

impl SolanaToCosmosTxBuilder {
    #[allow(clippy::too_many_arguments)]
    async fn relay_events(
        &self,
        solana_src_events: Vec<SolanaEurekaEventWithHeight>,
        target_events: Vec<EurekaEventWithHeight>,
        timeout_relay_height: Option<u64>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Real() => unreachable!(),
            Self::Mock(tb) => {
                tb.relay_events(
                    solana_src_events,
                    target_events,
                    src_client_id,
                    dst_client_id,
                    src_packet_seqs,
                    dst_packet_seqs,
                )
                .await
            }
            Self::Attested(tb) => {
                let src_events = solana_src_events
                    .into_iter()
                    .map(EurekaEventWithHeight::from)
                    .collect();
                tb.relay_events(RelayEventsParams {
                    src_events,
                    target_events,
                    timeout_relay_height,
                    src_client_id,
                    dst_client_id,
                    src_packet_seqs,
                    dst_packet_seqs,
                })
                .await
            }
        }
    }

    async fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Real() => unreachable!(),
            Self::Mock(tb) => tb.create_client(parameters).await,
            Self::Attested(tb) => tb.create_client(parameters),
        }
    }

    async fn update_client(&self, dst_client_id: String) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Real() => unreachable!(),
            Self::Mock(tb) => tb.update_client(dst_client_id).await,
            Self::Attested(tb) => tb.update_client(&dst_client_id).await,
        }
    }

    const fn is_attested(&self) -> bool {
        matches!(self, Self::Attested(_))
    }
}
