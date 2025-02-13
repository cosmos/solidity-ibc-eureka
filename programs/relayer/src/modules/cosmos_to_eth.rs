//! Defines Cosmos to Ethereum relayer module.

use std::str::FromStr;

use alloy::{
    primitives::{Address, TxHash},
    providers::{Provider, RootProvider},
};
use ibc_eureka_relayer_lib::{
    listener::{cosmos_sdk, eth_eureka, ChainListenerService},
    tx_builder::{cosmos_to_eth::TxBuilder, TxBuilderService},
};
use tendermint::Hash;
use tendermint_rpc::{HttpClient, Url};
use tonic::{Request, Response};

use crate::{
    api::{self, relayer_service_server::RelayerService},
    core::modules::ModuleServer,
};

/// The `CosmosToEthRelayerModule` struct defines the Cosmos to Ethereum relayer module.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct CosmosToEthRelayerModule;

/// The `CosmosToEthRelayerModuleServer` defines the relayer server from Cosmos to Ethereum.
struct CosmosToEthRelayerModuleServer {
    /// The chain listener for Cosmos SDK.
    pub tm_listener: cosmos_sdk::ChainListener,
    /// The chain listener for `EthEureka`.
    pub eth_listener: eth_eureka::ChainListener<RootProvider>,
    /// The transaction builder for `EthEureka`.
    pub tx_builder: TxBuilder<RootProvider>,
}

/// The configuration for the Cosmos to Ethereum relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::module_name_repetitions)]
pub struct CosmosToEthConfig {
    /// The tendermint RPC URL.
    pub tm_rpc_url: String,
    /// The ICS26 address.
    pub ics26_address: Address,
    /// The EVM RPC URL.
    pub eth_rpc_url: String,
    /// The SP1 prover network private key.
    #[serde(default)]
    pub sp1_private_key: Option<String>,
    /// Whether to run in mock mode.
    #[serde(default)]
    pub mock: bool,
}

impl CosmosToEthRelayerModuleServer {
    async fn new(config: CosmosToEthConfig) -> Self {
        let tm_client = HttpClient::new(
            Url::from_str(&config.tm_rpc_url)
                .unwrap_or_else(|_| panic!("invalid tendermint RPC URL: {}", config.tm_rpc_url)),
        )
        .expect("Failed to create tendermint HTTP client");

        let tm_listener = cosmos_sdk::ChainListener::new(tm_client.clone());

        let provider = RootProvider::builder()
            .on_builtin(&config.eth_rpc_url)
            .await
            .unwrap_or_else(|e| panic!("failed to create provider: {e}"));

        let eth_listener = eth_eureka::ChainListener::new(config.ics26_address, provider.clone());
        let submitter = if config.mock {
            TxBuilder::new_mock(config.ics26_address, provider, tm_client)
        } else {
            TxBuilder::new(
                config.ics26_address,
                provider,
                tm_client,
                config.sp1_private_key,
            )
        };

        Self {
            tm_listener,
            eth_listener,
            tx_builder: submitter,
        }
    }
}

#[tonic::async_trait]
impl RelayerService for CosmosToEthRelayerModuleServer {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Received info request.");
        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: self
                    .eth_listener
                    .chain_id()
                    .await
                    .map_err(|e| tonic::Status::from_error(e.to_string().into()))?,
                ibc_version: "2".to_string(),
                ibc_contract: self.tx_builder.ics26_router.address().to_string(),
            }),
            source_chain: Some(api::Chain {
                chain_id: self
                    .tm_listener
                    .chain_id()
                    .await
                    .map_err(|e| tonic::Status::from_error(e.to_string().into()))?,
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for cosmos to eth...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());
        let cosmos_txs = inner_req
            .source_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.to_string().into()))?;

        let eth_txs = inner_req
            .timeout_tx_ids
            .into_iter()
            .map(TryInto::<[u8; 32]>::try_into)
            .map(|tx_hash| tx_hash.map(TxHash::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|tx| tonic::Status::from_error(format!("invalid tx hash: {tx:?}").into()))?;

        let cosmos_events = self
            .tm_listener
            .fetch_tx_events(cosmos_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.to_string().into()))?;

        tracing::debug!(cosmos_events = ?cosmos_events, "Fetched cosmos events.");
        tracing::info!(
            "Fetched {} eureka events from CosmosSDK.",
            cosmos_events.len()
        );

        let eth_events = self
            .eth_listener
            .fetch_tx_events(eth_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.to_string().into()))?;

        tracing::debug!(eth_events = ?eth_events, "Fetched EVM events.");
        tracing::info!("Fetched {} eureka events from EVM.", eth_events.len());

        let multicall_tx = self
            .tx_builder
            .relay_events(cosmos_events, eth_events, inner_req.target_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.to_string().into()))?;

        tracing::info!("Relay by tx request completed.");

        Ok(Response::new(api::RelayByTxResponse {
            tx: multicall_tx,
            address: self.tx_builder.ics26_router.address().to_string(),
        }))
    }
}

#[tonic::async_trait]
impl ModuleServer for CosmosToEthRelayerModule {
    fn name(&self) -> &'static str {
        "cosmos_to_eth"
    }

    #[tracing::instrument(skip_all)]
    async fn serve(&self, config: serde_json::Value) -> anyhow::Result<Box<dyn RelayerService>> {
        let config = serde_json::from_value::<CosmosToEthConfig>(config)
            .map_err(|e| anyhow::anyhow!("failed to parse config: {e}"))?;

        tracing::info!("Starteing Cosmos to Ethereum relayer server.");
        Ok(Box::new(CosmosToEthRelayerModuleServer::new(config).await))
    }
}
