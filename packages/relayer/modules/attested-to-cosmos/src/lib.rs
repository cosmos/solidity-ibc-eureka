//! This is a one-sided relayer module from an Attested chain to a Cosmos SDK chain.

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

// Satisfy unused_crate_dependencies for transitive-only crates in this module
use alloy_primitives as _;
use attestor_packet_membership as _;

pub mod tx_builder;
mod tx_listener;

use std::collections::HashMap;

use alloy::{
    primitives::{Address, TxHash},
    providers::{Provider, RootProvider},
};
use ibc_eureka_relayer_lib::{
    listener::{cosmos_sdk, eth_eureka, ChainListenerService},
    tx_builder::TxBuilderService,
};
use ibc_eureka_utils::rpc::TendermintRpcExt;
use tendermint::Hash;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

use crate::tx_listener::{TxAdapter, TxListener};

/// The `AttestedToCosmosRelayerModule` struct defines the Attested to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
pub struct AttestedToCosmosRelayerModule;

/// The `AttestedToCosmosRelayerModuleService` defines the relayer service from Attested to Cosmos.
struct AttestedToCosmosRelayerModuleService<T>
where
    T: TxListener,
{
    /// The source chain ID for the attested chain.
    pub attested_chain_id: String,
    /// The chain listener for `EthEureka`.
    pub attestor_listener: T,
    /// The target chain listener for Cosmos SDK.
    pub tm_listener: cosmos_sdk::ChainListener,
    /// The transaction builder from Attested to Cosmos.
    pub tx_builder: tx_builder::TxBuilder,
}

/// The configuration for the Attested to Cosmos relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct AttestedToCosmosConfig {
    /// The source chain ID for the attested chain.
    pub attested_chain_id: String,
    /// The aggregator service URL for fetching attestations.
    pub aggregator_url: String,
    // TODO: Make this chain agnostic, see IBC-162
    /// The EVM RPC URL.
    pub attested_rpc_url: String,
    /// ICS26 address
    pub ics26_address: Address,
    /// The target tendermint RPC URL.
    pub tm_rpc_url: String,
    /// The address of the submitter.
    /// Required since cosmos messages require a signer address.
    pub signer_address: String,
}

impl
    AttestedToCosmosRelayerModuleService<
        eth_eureka::ChainListener<RootProvider<alloy::network::Ethereum>>,
    >
{
    pub async fn new(config: AttestedToCosmosConfig) -> Self {
        let provider = RootProvider::builder()
            .connect(&config.attested_rpc_url)
            .await
            .unwrap_or_else(|e| panic!("failed to create provider: {e}"));
        let attestor_listener = eth_eureka::ChainListener::new(config.ics26_address, provider);

        let target_client = HttpClient::from_rpc_url(&config.tm_rpc_url);
        let tm_listener = cosmos_sdk::ChainListener::new(target_client.clone());

        let tx_builder = tx_builder::TxBuilder::new(
            config.aggregator_url.clone(),
            target_client,
            config.signer_address,
        );

        Self {
            attested_chain_id: config.attested_chain_id,
            attestor_listener,
            tm_listener,
            tx_builder,
        }
    }
}

#[tonic::async_trait]
impl<T> RelayerService for AttestedToCosmosRelayerModuleService<T>
where
    T: TxListener,
{
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Attested to Cosmos...");
        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: self
                    .tm_listener
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

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Attested to Cosmos...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        let src_txs = inner_req
            .source_tx_ids
            .into_iter()
            .map(TryInto::<[u8; 32]>::try_into)
            .map(|tx_hash| tx_hash.map(TxHash::from))
            .map(|maybe_hashed| maybe_hashed.map(TxAdapter::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|tx| tonic::Status::from_error(format!("invalid tx hash: {tx:?}").into()))?;

        let cosmos_txs = inner_req
            .timeout_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.into()))?;

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

        let cosmos_events = self
            .tm_listener
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
                attested_events,
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
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Attested to Cosmos...");

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

    #[tracing::instrument(skip_all)]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Attested to Cosmos...");

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
impl RelayerModule for AttestedToCosmosRelayerModule {
    fn name(&self) -> &'static str {
        "attested_to_cosmos"
    }

    #[tracing::instrument(skip_all)]
    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config = serde_json::from_value::<AttestedToCosmosConfig>(config)
            .map_err(|e| anyhow::anyhow!("failed to parse config: {e}"))?;

        tracing::info!("Starting Attested to Cosmos relayer server.");
        Ok(Box::new(
            AttestedToCosmosRelayerModuleService::new(config).await,
        ))
    }
}

#[cfg(test)]
mod tests {
    use alloy::hex;

    use super::*;

    #[test]
    fn test_module_name() {
        let module = AttestedToCosmosRelayerModule;
        assert_eq!(module.name(), "attested_to_cosmos");
    }

    #[test]
    fn test_config_serialization() {
        let config = AttestedToCosmosConfig {
            aggregator_url: "http://localhost:8080".to_string(),
            ics26_address: Address(hex!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").into()),
            attested_rpc_url: "http://localhost:8080".to_string(),
            tm_rpc_url: "http://localhost:26657".to_string(),
            signer_address: "cosmos1abc123".to_string(),
            attested_chain_id: "attested-chain-1".to_string(),
        };

        let json = serde_json::to_string(&config).expect("Failed to serialize config");
        let deserialized: AttestedToCosmosConfig =
            serde_json::from_str(&json).expect("Failed to deserialize config");

        assert_eq!(config.aggregator_url, deserialized.aggregator_url);
        assert_eq!(config.tm_rpc_url, deserialized.tm_rpc_url);
        assert_eq!(config.signer_address, deserialized.signer_address);
        assert_eq!(config.attested_chain_id, deserialized.attested_chain_id);
    }
}
