//! Defines Cosmos to Ethereum relayer module.

use std::{net::SocketAddr, str::FromStr};

use alloy::{
    network::{Ethereum, EthereumWallet},
    primitives::{Address, TxHash},
    providers::{
        fillers::{FillProvider, JoinFill, WalletFiller},
        Identity, ProviderBuilder, RootProvider,
    },
    signers::local::PrivateKeySigner,
    transports::BoxTransport,
};
use ibc_eureka_relayer_lib::{
    listener::{cosmos_sdk, eth_eureka, ChainListenerService},
    tx_builder::{
        eth_eureka::{SupportedProofType, TxBuilder},
        TxBuilderService,
    },
};
use tendermint::Hash;
use tendermint_rpc::{HttpClient, Url};
use tonic::{transport::Server, Request, Response};

use crate::{
    api::{
        self,
        relayer_service_server::{RelayerService, RelayerServiceServer},
    },
    core::modules::{RelayerModule, RelayerModuleServer},
};

type Provider = FillProvider<
    JoinFill<Identity, WalletFiller<EthereumWallet>>,
    RootProvider<BoxTransport>,
    BoxTransport,
    Ethereum,
>;

/// The `RelayerModule` defines the relayer module for Cosmos to Ethereum.
#[allow(clippy::module_name_repetitions)]
pub struct CosmosToEthRelayerModule {
    /// The chain listener for Cosmos SDK.
    pub tm_listener: cosmos_sdk::ChainListener,
    /// The chain listener for `EthEureka`.
    pub eth_listener: eth_eureka::ChainListener<BoxTransport, Provider>,
    /// The chain submitter for `EthEureka`.
    pub submitter: TxBuilder<BoxTransport, Provider>,
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
    /// The private key for the Ethereum account.
    // TODO: Use a more secure way to store the private key.
    pub private_key: String,
    /// The proof type to use for the SP1 ICS07 Tendermint prover.
    /// This is either groth16 or plonk.
    pub proof_type: String,
    /// The SP1 prover network private key.
    pub sp1_private_key: String,
}

#[tonic::async_trait]
impl RelayerModule for CosmosToEthRelayerModule {
    type Config = CosmosToEthConfig;

    const NAME: &'static str = "cosmos_to_eth";

    async fn new(config: Self::Config) -> Self {
        let tm_client = HttpClient::new(
            Url::from_str(&config.tm_rpc_url)
                .unwrap_or_else(|_| panic!("invalid tendermint RPC URL: {}", config.tm_rpc_url)),
        )
        .expect("Failed to create tendermint HTTP client");

        let tm_listener = cosmos_sdk::ChainListener::new(tm_client.clone());

        let wallet = EthereumWallet::from(
            config
                .private_key
                .strip_prefix("0x")
                .unwrap_or(&config.private_key)
                .parse::<PrivateKeySigner>()
                .expect("Failed to parse private key"),
        );

        let provider = ProviderBuilder::new()
            .wallet(wallet.clone())
            .on_builtin(&config.eth_rpc_url)
            .await
            .unwrap_or_else(|e| panic!("failed to create provider: {e}"));

        let eth_listener = eth_eureka::ChainListener::new(config.ics26_address, provider.clone());
        let submitter = TxBuilder::new(
            config.ics26_address,
            provider,
            tm_client,
            config.proof_type(),
            Some(config.sp1_private_key),
        );

        Self {
            tm_listener,
            eth_listener,
            submitter,
        }
    }
}

#[tonic::async_trait]
impl RelayerService for CosmosToEthRelayerModule {
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

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request...");
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

        tracing::info!("Relay by tx request completed.");

        Ok(Response::new(api::RelayByTxResponse {
            tx: multicall_tx,
            address: self.submitter.ics26_router.address().to_string(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModuleServer for CosmosToEthRelayerModule {
    #[tracing::instrument(skip_all)]
    async fn serve(self: Box<Self>, addr: SocketAddr) -> Result<(), tonic::transport::Error> {
        tracing::info!(%addr, "Started Cosmos to Ethereum relayer server.");

        Server::builder()
            .add_service(RelayerServiceServer::new(*self))
            .serve(addr)
            .await
    }
}

impl CosmosToEthConfig {
    /// Parses the proof type from the configuration.
    /// # Panics
    /// Panics if the proof type is not recognized.
    #[must_use]
    pub fn proof_type(&self) -> SupportedProofType {
        match self.proof_type.as_str() {
            "groth16" => SupportedProofType::Groth16,
            "plonk" => SupportedProofType::Plonk,
            _ => panic!("invalid proof type: {}", self.proof_type),
        }
    }
}
