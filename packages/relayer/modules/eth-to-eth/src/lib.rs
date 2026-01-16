//! This is a one-sided relayer module from Ethereum to Ethereum.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(unused_crate_dependencies, missing_docs)]

use std::collections::HashMap;

use alloy::{
    primitives::{Address, TxHash},
    providers::{Provider, RootProvider},
};
use ibc_eureka_relayer_lib::{
    aggregator::{Aggregator, Config as AggregatorConfig},
    events::EurekaEventWithHeight,
    listener::{eth_eureka, ChainListenerService},
    utils::eth_attested::{
        build_eth_attestor_create_client_calldata, build_eth_attestor_relay_events_tx,
    },
};
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

/// The `EthToEthRelayerModule` struct defines the Ethereum to Ethereum relayer module.
#[derive(Clone, Copy, Debug)]
pub struct EthToEthRelayerModule;

/// The `EthToEthRelayerModuleService` defines the relayer service from Ethereum to Ethereum.
struct EthToEthRelayerModuleService {
    /// The source chain ID.
    src_chain_id: String,
    /// The source ICS26 address.
    src_ics26_address: Address,
    /// The chain listener for source Ethereum.
    src_listener: eth_eureka::ChainListener<RootProvider>,
    /// The chain listener for destination Ethereum.
    dst_listener: eth_eureka::ChainListener<RootProvider>,
    /// The transaction builder.
    tx_builder: EthToEthTxBuilder,
}

enum EthToEthTxBuilder {
    Attested(AttestedTxBuilder),
}

/// Transaction builder for attested relay from Ethereum to Ethereum.
struct AttestedTxBuilder {
    aggregator: Aggregator,
    ics26_address: Address,
    provider: RootProvider,
}

/// The configuration for the Ethereum to Ethereum relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct EthToEthConfig {
    /// The source chain ID.
    pub src_chain_id: String,
    /// The source Ethereum RPC URL.
    pub src_rpc_url: String,
    /// The source ICS26 address.
    pub src_ics26_address: Address,
    /// The destination Ethereum RPC URL.
    pub dst_rpc_url: String,
    /// The destination ICS26 address.
    pub dst_ics26_address: Address,
    /// Transaction builder mode.
    pub mode: TxBuilderMode,
}

/// Transaction builder mode configuration.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TxBuilderMode {
    /// Attested mode using aggregator attestations.
    Attested {
        /// Aggregator configuration.
        aggregator_config: AggregatorConfig,
    },
}

impl EthToEthRelayerModuleService {
    async fn new(config: EthToEthConfig) -> anyhow::Result<Self> {
        let src_provider = RootProvider::builder()
            .connect(&config.src_rpc_url)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create source provider: {e}"))?;

        let dst_provider = RootProvider::builder()
            .connect(&config.dst_rpc_url)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create destination provider: {e}"))?;

        let src_listener = eth_eureka::ChainListener::new(config.src_ics26_address, src_provider);
        let dst_listener =
            eth_eureka::ChainListener::new(config.dst_ics26_address, dst_provider.clone());

        let tx_builder = match config.mode {
            TxBuilderMode::Attested { aggregator_config } => {
                let aggregator = Aggregator::from_config(aggregator_config)
                    .await
                    .map_err(|e| anyhow::anyhow!("failed to create aggregator: {e}"))?;
                EthToEthTxBuilder::Attested(AttestedTxBuilder {
                    aggregator,
                    ics26_address: config.dst_ics26_address,
                    provider: dst_provider,
                })
            }
        };

        Ok(Self {
            src_chain_id: config.src_chain_id,
            src_ics26_address: config.src_ics26_address,
            src_listener,
            dst_listener,
            tx_builder,
        })
    }
}

fn to_tonic_status(e: anyhow::Error) -> tonic::Status {
    tonic::Status::from_error(e.into())
}

#[allow(clippy::result_large_err)]
fn parse_eth_tx_hashes(tx_ids: Vec<Vec<u8>>) -> Result<Vec<TxHash>, tonic::Status> {
    tx_ids
        .into_iter()
        .map(TryInto::<[u8; 32]>::try_into)
        .map(|tx_hash| tx_hash.map(TxHash::from))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|tx| tonic::Status::from_error(format!("invalid tx hash: {tx:?}").into()))
}

#[tonic::async_trait]
impl RelayerService for EthToEthRelayerModuleService {
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Eth to Eth...");

        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: self
                    .dst_listener
                    .chain_id()
                    .await
                    .map_err(to_tonic_status)?,
                ibc_version: "2".to_string(),
                ibc_contract: self.tx_builder.ics26_router_address().to_string(),
            }),
            source_chain: Some(api::Chain {
                chain_id: self.src_chain_id.clone(),
                ibc_version: "2".to_string(),
                ibc_contract: self.src_ics26_address.to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Eth to Eth...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        let src_txs = parse_eth_tx_hashes(inner_req.source_tx_ids)?;
        let timeout_txs = parse_eth_tx_hashes(inner_req.timeout_tx_ids)?;

        let src_events = self
            .src_listener
            .fetch_tx_events(src_txs)
            .await
            .map_err(to_tonic_status)?;

        tracing::debug!(?src_events, "Fetched source Eth events.");
        tracing::info!(
            "Fetched {} eureka events from source Eth.",
            src_events.len()
        );

        let has_timeouts = !timeout_txs.is_empty();

        let dst_events = self
            .dst_listener
            .fetch_tx_events(timeout_txs)
            .await
            .map_err(to_tonic_status)?;

        tracing::debug!(?dst_events, "Fetched destination Eth events.");
        tracing::info!(
            "Fetched {} eureka events from destination Eth.",
            dst_events.len()
        );

        // For timeouts, get the current height from the source chain (where non-membership is proven)
        let timeout_relay_height = if has_timeouts {
            Some(
                self.src_listener
                    .get_block_number()
                    .await
                    .map_err(to_tonic_status)?,
            )
        } else {
            None
        };

        let tx = self
            .tx_builder
            .relay_events(
                src_events,
                dst_events,
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
            address: self.tx_builder.ics26_router_address().to_string(),
        }))
    }

    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Eth to Eth...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .create_client(&inner_req.parameters)
            .map_err(to_tonic_status)?;

        tracing::info!("Create client request completed.");

        Ok(Response::new(api::CreateClientResponse {
            tx,
            address: String::new(),
        }))
    }

    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Eth to Eth...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .update_client(inner_req.dst_client_id)
            .await
            .map_err(to_tonic_status)?;

        tracing::info!("Update client request completed.");

        Ok(Response::new(api::UpdateClientResponse {
            tx,
            address: self.tx_builder.ics26_router_address().to_string(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for EthToEthRelayerModule {
    fn name(&self) -> &'static str {
        "eth_to_eth"
    }

    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config: EthToEthConfig = serde_json::from_value(config)?;
        let service = EthToEthRelayerModuleService::new(config).await?;
        Ok(Box::new(service))
    }
}

impl EthToEthTxBuilder {
    async fn relay_events(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        target_events: Vec<EurekaEventWithHeight>,
        timeout_relay_height: Option<u64>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Attested(tb) => {
                build_eth_attestor_relay_events_tx(
                    &tb.aggregator,
                    src_events,
                    target_events,
                    timeout_relay_height,
                    src_client_id,
                    dst_client_id,
                    src_packet_seqs,
                    dst_packet_seqs,
                )
                .await
            }
        }
    }

    fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Attested(tb) => {
                build_eth_attestor_create_client_calldata(parameters, tb.provider.clone())
            }
        }
    }

    #[allow(clippy::pedantic)]
    async fn update_client(&self, _dst_client_id: String) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Attested(_) => {
                // TODO: IBC-164
                todo!()
            }
        }
    }

    const fn ics26_router_address(&self) -> &Address {
        match self {
            Self::Attested(tb) => &tb.ics26_address,
        }
    }
}
