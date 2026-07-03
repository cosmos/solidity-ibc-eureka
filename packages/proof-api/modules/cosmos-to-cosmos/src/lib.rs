//! This is a one-sided proof API module from a Cosmos SDK chain to a Cosmos SDK chain.

#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

use ics23 as _;
use tendermint as _;

pub mod tx_builder;

use std::collections::HashMap;

use ibc_eureka_utils::rpc::TendermintRpcExt;
use proof_api_lib::{
    aggregator::{Aggregator, Config as AggregatorConfig},
    listener::{cosmos_sdk, ChainListenerService},
    service_utils::{parse_cosmos_tx_hashes, to_tonic_status},
    tx_builder::TxBuilderService,
    utils::RelayEventsParams,
};
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use proof_api_core::{
    api::{self, proof_api_service_server::ProofApiService},
    modules::ProofApiModule,
};

/// The `CosmosToCosmosProofApiModule` struct defines the Cosmos to Cosmos proof API module.
#[derive(Clone, Copy, Debug)]
pub struct CosmosToCosmosProofApiModule;

/// The `CosmosToCosmosProofApiModuleService` defines the proof API service from Cosmos to Cosmos.
struct CosmosToCosmosProofApiModuleService {
    /// The souce chain listener for Cosmos SDK.
    pub src_listener: cosmos_sdk::ChainListener,
    /// The target chain listener for Cosmos SDK.
    pub target_listener: cosmos_sdk::ChainListener,
    /// The transaction builder from Cosmos to Cosmos.
    pub tx_builder: CosmosToCosmosTxBuilder,
}

/// The transaction builder variants for the Cosmos to Cosmos module.
enum CosmosToCosmosTxBuilder {
    /// Native `07-tendermint` client: headers and merkle proofs.
    Native(tx_builder::TxBuilder),
    /// `attestations` client: aggregator attestations (e.g. sandbox-ledger).
    Attested(tx_builder::AttestedTxBuilder),
}

/// The configuration for the Cosmos to Cosmos proof API module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CosmosToCosmosConfig {
    /// The source tendermint RPC URL.
    pub src_rpc_url: String,
    /// The target tendermint RPC URL.
    pub target_rpc_url: String,
    /// The address of the submitter.
    /// Required since cosmos messages require a signer address.
    pub signer_address: String,
    /// Transaction builder mode. Defaults to the native `07-tendermint` client
    /// so existing configs without this key keep working.
    #[serde(default)]
    pub mode: TxBuilderMode,
}

/// Transaction builder mode configuration.
#[derive(Clone, Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TxBuilderMode {
    /// Native mode using `07-tendermint` headers and merkle proofs.
    #[default]
    Native,
    /// Attested mode using aggregator attestations (for chains whose only
    /// counterparty light client is `attestations`, such as sandbox-ledger).
    Attested(AggregatorConfig),
}

impl CosmosToCosmosProofApiModuleService {
    async fn new(config: CosmosToCosmosConfig) -> anyhow::Result<Self> {
        let src_client = HttpClient::from_rpc_url(&config.src_rpc_url);
        let src_listener = cosmos_sdk::ChainListener::new(src_client.clone());
        let target_client = HttpClient::from_rpc_url(&config.target_rpc_url);
        let target_listener = cosmos_sdk::ChainListener::new(target_client.clone());

        let tx_builder =
            match config.mode {
                TxBuilderMode::Native => CosmosToCosmosTxBuilder::Native(
                    tx_builder::TxBuilder::new(src_client, target_client, config.signer_address),
                ),
                TxBuilderMode::Attested(aggregator_config) => {
                    let aggregator = Aggregator::from_config(aggregator_config)
                        .await
                        .map_err(|e| anyhow::anyhow!("failed to create aggregator: {e}"))?;
                    CosmosToCosmosTxBuilder::Attested(tx_builder::AttestedTxBuilder::new(
                        aggregator,
                        config.signer_address,
                    ))
                }
            };

        Ok(Self {
            src_listener,
            target_listener,
            tx_builder,
        })
    }
}

impl CosmosToCosmosTxBuilder {
    const fn is_attested(&self) -> bool {
        matches!(self, Self::Attested(_))
    }

    async fn relay_events(&self, params: RelayEventsParams) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Native(tb) => {
                tb.relay_events(
                    params.src_events,
                    params.target_events,
                    params.src_client_id,
                    params.dst_client_id,
                    params.src_packet_seqs,
                    params.dst_packet_seqs,
                )
                .await
            }
            Self::Attested(tb) => tb.relay_events(params).await,
        }
    }

    async fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Native(tb) => tb.create_client(parameters).await,
            Self::Attested(tb) => tb.create_client(parameters),
        }
    }

    async fn update_client(&self, dst_client_id: String) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Native(tb) => tb.update_client(dst_client_id).await,
            Self::Attested(tb) => tb.update_client(&dst_client_id).await,
        }
    }
}

#[tonic::async_trait]
impl ProofApiService for CosmosToCosmosProofApiModuleService {
    #[tracing::instrument(skip_all, err(Debug))]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Cosmos to Cosmos...");
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
                chain_id: self
                    .src_listener
                    .chain_id()
                    .await
                    .map_err(to_tonic_status)?,
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            metadata: HashMap::default(),
        }))
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Cosmos to Cosmos...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());
        let src_txs = parse_cosmos_tx_hashes(inner_req.source_tx_ids)?;

        let target_txs = parse_cosmos_tx_hashes(inner_req.timeout_tx_ids)?;

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

        tracing::debug!(cosmos_target_events = ?target_events, "Fetched target cosmos events.");
        tracing::info!(
            "Fetched {} target eureka events from CosmosSDK.",
            target_events.len()
        );

        // For timeouts in attested mode, non-membership is proven against the
        // source chain's current height, so the attestation must be taken there.
        let timeout_relay_height = if self.tx_builder.is_attested() && !target_events.is_empty() {
            Some(
                self.src_listener
                    .get_block_height()
                    .await
                    .map_err(|e| tonic::Status::from_error(e.into()))?,
            )
        } else {
            None
        };

        let tx = self
            .tx_builder
            .relay_events(RelayEventsParams {
                src_events,
                target_events,
                timeout_relay_height,
                src_client_id: inner_req.src_client_id,
                dst_client_id: inner_req.dst_client_id,
                src_packet_seqs: inner_req.src_packet_sequences,
                dst_packet_seqs: inner_req.dst_packet_sequences,
            })
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Relay by tx request completed.");

        Ok(Response::new(api::RelayByTxResponse {
            tx,
            address: String::new(),
        }))
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Cosmos to Cosmos...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .create_client(&inner_req.parameters)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Create client request completed.");

        Ok(Response::new(api::CreateClientResponse {
            tx,
            address: String::new(),
        }))
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Cosmos to Cosmos...");

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
        }))
    }
}

#[tonic::async_trait]
impl ProofApiModule for CosmosToCosmosProofApiModule {
    fn name(&self) -> &'static str {
        "cosmos_to_cosmos"
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn ProofApiService>> {
        let config = serde_json::from_value::<CosmosToCosmosConfig>(config)
            .map_err(|e| anyhow::anyhow!("failed to parse config: {e}"))?;

        tracing::info!("Starting Cosmos to Cosmos proof API server.");
        Ok(Box::new(
            CosmosToCosmosProofApiModuleService::new(config).await?,
        ))
    }
}
