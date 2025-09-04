//! This is a one-sided relayer module from an Attested Cosmos SDK chain to Ethereum.

#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]
#![allow(
    clippy::doc_markdown,
    clippy::derive_partial_eq_without_eq,
    clippy::missing_errors_doc
)]

pub mod tx_builder;

use std::collections::HashMap;

use alloy::{
    primitives::{Address, TxHash},
    providers::{Provider, RootProvider},
};
use ibc_eureka_relayer_lib::{
    aggregator::Config,
    listener::{cosmos_sdk, eth_eureka, ChainListenerService},
    tx_builder::TxBuilderService,
};
use ibc_eureka_utils::rpc::TendermintRpcExt;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

/// The `CosmosToEthAttestedRelayerModule` struct defines the relayer module.
#[derive(Clone, Copy, Debug)]
pub struct CosmosToEthAttestedRelayerModule;

/// The `CosmosToEthAttestedRelayerModuleService` defines the relayer service.
struct CosmosToEthAttestedRelayerModuleService {
    /// The source chain ID for the attested chain.
    pub attested_chain_id: String,
    /// The chain listener for `Cosmos`.
    pub attestor_listener: cosmos_sdk::ChainListener,
    /// The `Ethereum` chain listener
    pub eth_listener: eth_eureka::ChainListener<RootProvider>,
    /// The transaction builder for `Cosmos` to `Ethereum`
    pub tx_builder: tx_builder::TxBuilder<RootProvider>,
}

/// The configuration for the Cosmos to Ethereum attested relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CosmosToEthAttestedConfig {
    /// The source chain ID for the attested chain.
    pub attested_chain_id: String,
    /// The aggregator service URL for fetching attestations.
    pub aggregator_config: Config,
    /// Attested RPC address for the cosmos chain
    pub attested_rpc_url: String,
    /// ICS26 address
    pub ics26_address: Address,
    /// The target ETH RPC address
    pub eth_rpc_url: String,
}

impl CosmosToEthAttestedRelayerModuleService {
    pub async fn new(config: CosmosToEthAttestedConfig) -> Self {
        let tm_client = HttpClient::from_rpc_url(&config.attested_rpc_url);
        let attestor_listener = cosmos_sdk::ChainListener::new(tm_client.clone());

        let provider = RootProvider::builder()
            .connect(&config.eth_rpc_url)
            .await
            .unwrap_or_else(|e| panic!("failed to create provider: {e}"));

        let eth_listener = eth_eureka::ChainListener::new(config.ics26_address, provider.clone());

        let tx_builder =
            tx_builder::TxBuilder::new(config.ics26_address, provider, config.aggregator_config)
                .await
                .expect("tx builder requires aggregator");

        Self {
            attested_chain_id: config.attested_chain_id,
            attestor_listener,
            eth_listener,
            tx_builder,
        }
    }
}

#[tonic::async_trait]
impl RelayerService for CosmosToEthAttestedRelayerModuleService {
    #[tracing::instrument(skip(self))]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: self
                    .eth_listener
                    .chain_id()
                    .await
                    .map_err(|e| tonic::Status::from_error(e.into()))?,
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            source_chain: Some(api::Chain {
                chain_id: self.attested_chain_id.clone(),
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            metadata: HashMap::default(),
        }))
    }

    #[tracing::instrument(skip(self))]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        let src_txs = inner_req
            .source_tx_ids
            .into_iter()
            .map(tendermint::Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(format!("invalid src tx hash {e}").into()))?;

        let eth_events = inner_req
            .timeout_tx_ids
            .into_iter()
            .map(TryInto::<[u8; 32]>::try_into)
            .map(|tx_hash| tx_hash.map(TxHash::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|tx| {
                tonic::Status::from_error(format!("invalid timeout tx hash: {tx:?}").into())
            })?;

        let attested_events = self
            .attestor_listener
            .fetch_tx_events(src_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(attested_events = ?attested_events, "Fetched attested events.");
        tracing::info!(
            "Fetched {} eureka events from attested chain.",
            attested_events.len()
        );

        let eth_events = self
            .eth_listener
            .fetch_tx_events(eth_events)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(eth_events = ?eth_events, "Fetched Eth events.");
        tracing::info!(
            "Fetched {} eureka events from Eth Listener.",
            eth_events.len()
        );

        let tx = self
            .tx_builder
            .relay_events(
                attested_events,
                eth_events,
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
        }))
    }

    #[tracing::instrument(skip(self))]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
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

    #[tracing::instrument(skip(self))]
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

        tracing::info!("Update client request completed.");

        Ok(Response::new(api::UpdateClientResponse {
            tx,
            address: String::new(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for CosmosToEthAttestedRelayerModule {
    fn name(&self) -> &'static str {
        "cosmos_to_eth_attested"
    }

    #[tracing::instrument(skip_all)]
    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config = serde_json::from_value::<CosmosToEthAttestedConfig>(config)
            .map_err(|e| anyhow::anyhow!("failed to parse config: {e}"))?;

        tracing::info!("Starting Cosmos to Eth attested relayer server.");
        Ok(Box::new(
            CosmosToEthAttestedRelayerModuleService::new(config).await,
        ))
    }
}

#[cfg(test)]
mod tests {
    use alloy::hex;
    use ibc_eureka_relayer_lib::aggregator::{AttestorConfig, CacheConfig};

    use super::*;

    #[test]
    fn test_module_name() {
        let module = CosmosToEthAttestedRelayerModule;
        assert_eq!(module.name(), "cosmos_to_eth_attested");
    }

    #[test]
    fn test_config_serialization() {
        let agg_config = Config {
            attestor: AttestorConfig {
                attestor_query_timeout_ms: 100_000,
                quorum_threshold: 1,
                attestor_endpoints: ["127.0.0.1:8080".to_string()].to_vec(),
            },
            cache: CacheConfig::default(),
        };
        let config = CosmosToEthAttestedConfig {
            aggregator_config: agg_config,
            ics26_address: Address(hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").into()),
            attested_rpc_url: "http://localhost:8080".to_string(),
            eth_rpc_url: "http://localhost:26657".to_string(),
            attested_chain_id: "attested-chain-1".to_string(),
        };

        let json = serde_json::to_string(&config).expect("Failed to serialize config");
        let deserialized: CosmosToEthAttestedConfig =
            serde_json::from_str(&json).expect("Failed to deserialize config");

        assert_eq!(config.eth_rpc_url, deserialized.eth_rpc_url);
        assert_eq!(config.attested_chain_id, deserialized.attested_chain_id);
    }
}
