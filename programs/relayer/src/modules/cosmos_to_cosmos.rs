//! Defines Cosmos to Cosmos relayer module.

use std::str::FromStr;

use ibc_eureka_relayer_lib::{listener::cosmos_sdk, tx_builder::cosmos_to_cosmos};
use tendermint_rpc::{HttpClient, Url};

/// The `CosmosToCosmosRelayerModule` struct defines the Cosmos to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct CosmosToCosmosRelayerModule;

/// The `CosmosToCosmosRelayerModuleServer` defines the relayer server from Cosmos to Cosmos.
#[allow(dead_code)]
struct CosmosToCosmosRelayerModuleServer {
    /// The souce chain listener for Cosmos SDK.
    pub src_listener: cosmos_sdk::ChainListener,
    /// The target chain listener for Cosmos SDK.
    pub target_listener: cosmos_sdk::ChainListener,
    /// The transaction builder from Cosmos to Cosmos.
    pub tx_builder: cosmos_to_cosmos::TxBuilder,
}

/// The configuration for the Cosmos to Cosmos relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[allow(clippy::module_name_repetitions)]
pub struct CosmosToCosmosConfig {
    /// The source tendermint RPC URL.
    pub src_rpc_url: String,
    /// The target tendermint RPC URL.
    pub target_rpc_url: String,
    /// The address of the submitter.
    /// Required since cosmos messages require a signer address.
    pub signer_address: String,
}

impl CosmosToCosmosRelayerModuleServer {
    #[allow(dead_code)]
    fn new(config: CosmosToCosmosConfig) -> Self {
        let src_client = HttpClient::new(
            Url::from_str(&config.src_rpc_url)
                .unwrap_or_else(|_| panic!("invalid tendermint RPC URL: {}", config.src_rpc_url)),
        )
        .expect("Failed to create tendermint HTTP client");

        let src_listener = cosmos_sdk::ChainListener::new(src_client.clone());

        let target_client =
            HttpClient::new(Url::from_str(&config.target_rpc_url).unwrap_or_else(|_| {
                panic!("invalid tendermint RPC URL: {}", config.target_rpc_url)
            }))
            .expect("Failed to create tendermint HTTP client");

        let target_listener = cosmos_sdk::ChainListener::new(target_client.clone());

        let tx_builder =
            cosmos_to_cosmos::TxBuilder::new(src_client, target_client, config.signer_address);

        Self {
            src_listener,
            target_listener,
            tx_builder,
        }
    }
}
