//! Defines Cosmos to Ethereum relayer module.

use alloy::{
    primitives::{Address, TxHash},
    providers::Provider,
    transports::Transport,
};
use ibc_eureka_relayer_lib::{
    listener::{cosmos_sdk, eth_eureka, ChainListenerService},
    submitter::{eth_eureka::ChainSubmitter, ChainSubmitterService},
};
use tendermint::Hash;
use tonic::{Request, Response};

use crate::api::{self, relayer_service_server::RelayerService};

use super::r#trait::RelayerModule;

/// The `RelayerModule` defines the relayer module for Cosmos to Ethereum.
#[allow(clippy::module_name_repetitions)]
pub struct CosmosToEthRelayerModule<T: Transport + Clone, P: Provider<T> + Clone> {
    /// The chain listener for Cosmos SDK.
    pub tm_listener: cosmos_sdk::ChainListener,
    /// The chain listener for `EthEureka`.
    pub eth_listener: eth_eureka::ChainListener<T, P>,
    /// The chain submitter for `EthEureka`.
    pub submitter: ChainSubmitter<T, P>,
}

/// The configuration for the Cosmos to Ethereum relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct RelayerConfig {
    /// The tendermint RPC URL.
    pub tm_rpc_url: String,
    /// The ICS26 address.
    pub ics26_address: Address,
    /// The EVM RPC URL.
    pub eth_rpc_url: String,
}

impl<T: Transport + Clone, P: Provider<T> + Clone + 'static> RelayerModule
    for CosmosToEthRelayerModule<T, P>
{
    type Config = RelayerConfig;

    const NAME: &'static str = "cosmos_to_eth";

    fn new(_config: Self::Config) -> Self {
        todo!()
    }
}

#[tonic::async_trait]
impl<T: Transport + Clone, P: Provider<T> + Clone + 'static> RelayerService
    for CosmosToEthRelayerModule<T, P>
{
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
                    .map_err(|e| tonic::Status::from_error(e.to_string().into()))?,
                ibc_version: "2".to_string(),
                ibc_contract: self.submitter.ics26_router.address().to_string(),
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

    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        let inner_req = request.into_inner();
        let cosmos_txs = inner_req
            .source_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.to_string().into()))?;

        let eth_txs = inner_req
            .target_tx_ids
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

        let eth_events = self
            .eth_listener
            .fetch_tx_events(eth_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.to_string().into()))?;

        let multicall_tx = self
            .submitter
            .relay_events(cosmos_events, eth_events, inner_req.target_channel_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.to_string().into()))?;

        Ok(Response::new(api::RelayByTxResponse {
            tx: multicall_tx,
            address: self.submitter.ics26_router.address().to_string(),
        }))
    }
}
