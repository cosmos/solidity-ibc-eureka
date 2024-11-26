//! Defines Cosmos to Ethereum relayer module.

use alloy::{primitives::Address, providers::Provider, transports::Transport};
use ibc_eureka_relayer_lib::{
    listener::{cosmos_sdk, eth_eureka},
    submitter::eth_eureka::ChainSubmitter,
};
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
                    .submitter
                    .ics26_router
                    .provider()
                    .get_chain_id()
                    .await
                    .map_err(|e| tonic::Status::from_error(e.to_string().into()))?
                    .to_string(),
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
        _request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        todo!()
    }
}
