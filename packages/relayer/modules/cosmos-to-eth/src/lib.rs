//! This is a one-sided relayer module from a Cosmos SDK chain to Ethereum.

#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

pub mod tx_builder;

use alloy::{
    primitives::{Address, TxHash},
    providers::{Provider, RootProvider},
};
use ibc_eureka_relayer_lib::{
    listener::{cosmos_sdk, eth_eureka, ChainListenerService},
    tx_builder::TxBuilderService,
};
use ibc_eureka_utils::rpc::TendermintRpcExt;
use sp1_ics07_tendermint_prover::programs::{
    MembershipProgram, MisbehaviourProgram, SP1ICS07TendermintPrograms,
    UpdateClientAndMembershipProgram, UpdateClientProgram,
};
use sp1_prover::components::CpuProverComponents;
use sp1_sdk::{Prover, ProverClient};
use tendermint::Hash;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};
use tx_builder::TxBuilder;

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

/// The `CosmosToEthRelayerModule` struct defines the Cosmos to Ethereum relayer module.
#[derive(Clone, Copy, Debug)]
pub struct CosmosToEthRelayerModule;

/// The `CosmosToEthRelayerModuleService` defines the relayer service from Cosmos to Ethereum.
struct CosmosToEthRelayerModuleService {
    /// The chain listener for Cosmos SDK.
    pub tm_listener: cosmos_sdk::ChainListener,
    /// The chain listener for `EthEureka`.
    pub eth_listener: eth_eureka::ChainListener<RootProvider>,
    /// The transaction builder for `EthEureka`.
    pub tx_builder: TxBuilder<RootProvider, CpuProverComponents>,
}

/// The configuration for the Cosmos to Ethereum relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CosmosToEthConfig {
    /// The tendermint RPC URL.
    pub tm_rpc_url: String,
    /// The ICS26 address.
    pub ics26_address: Address,
    /// The EVM RPC URL.
    pub eth_rpc_url: String,
    /// The SP1 prover configuration.
    pub sp1_prover: SP1Config,
    /// The SP1 program paths.
    pub sp1_programs: SP1ProgramPaths,
}

/// The paths to the SP1 programs.
/// This is relative to the current working directory.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SP1ProgramPaths {
    /// The path to the update client program.
    pub update_client: String,
    /// The path to the membership program.
    pub membership: String,
    /// The path to the update client and membership program.
    pub update_client_and_membership: String,
    /// The path to the misbehaviour program.
    pub misbehaviour: String,
}

impl SP1ProgramPaths {
    /// Get the ELF bytes for the programs.
    ///
    /// # Errors
    /// Returns an error if the programs cannot be read.
    pub fn read_programs(&self) -> anyhow::Result<SP1ICS07TendermintPrograms> {
        let update_client = std::fs::read(&self.update_client)?;
        let membership = std::fs::read(&self.membership)?;
        let uc_and_membership = std::fs::read(&self.update_client_and_membership)?;
        let misbehaviour = std::fs::read(&self.misbehaviour)?;

        Ok(SP1ICS07TendermintPrograms {
            update_client: UpdateClientProgram::new(update_client),
            membership: MembershipProgram::new(membership),
            update_client_and_membership: UpdateClientAndMembershipProgram::new(uc_and_membership),
            misbehaviour: MisbehaviourProgram::new(misbehaviour),
        })
    }
}

/// The configuration for the SP1 prover.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SP1Config {
    /// Mock prover.
    Mock,
    /// Prover from environment variables. (Usually a network prover)
    ///
    /// `sp1-sdk` expects the following environment variables:
    /// - `SP1_PROVER`: The prover type. (mock, network, cpu, cuda)
    /// - `NETWORK_PRIVATE_KEY`: The private key for the network prover. (only network)
    /// - `NETWORK_RPC_URL`: The RPC URL for prover network. (only network, default exists if empty)
    Env,
    /// The network prover.
    Network {
        /// The optional private key for the network prover.
        /// `NETWORK_PRIVATE_KEY` environment variable is used if not provided.
        #[serde(default)]
        network_private_key: Option<String>,
        /// The optional RPC URL for the network prover.
        /// `NETWORK_RPC_URL` environment variable is used if not provided.
        #[serde(default)]
        network_rpc_url: Option<String>,
        /// Whether to use a private cluster.
        #[serde(default)]
        private_cluster: bool,
    },
    /// The local CPU prover.
    Cpu,
    /// The local CUDA prover.
    Cuda,
}

impl CosmosToEthRelayerModuleService {
    async fn new(config: CosmosToEthConfig) -> Self {
        let tm_client = HttpClient::from_rpc_url(&config.tm_rpc_url);
        let tm_listener = cosmos_sdk::ChainListener::new(tm_client.clone());

        let provider = RootProvider::builder()
            .connect(&config.eth_rpc_url)
            .await
            .unwrap_or_else(|e| panic!("failed to create provider: {e}"));

        let eth_listener = eth_eureka::ChainListener::new(config.ics26_address, provider.clone());

        let sp1_programs = config
            .sp1_programs
            .read_programs()
            .unwrap_or_else(|e| panic!("failed to read SP1 programs: {e}"));

        let tx_builder = match config.sp1_prover {
            SP1Config::Mock => {
                let prover: Box<dyn Prover<CpuProverComponents>> =
                    Box::new(ProverClient::builder().mock().build());
                TxBuilder::new(
                    config.ics26_address,
                    provider,
                    tm_client,
                    prover,
                    sp1_programs,
                )
            }
            SP1Config::Env => {
                let prover: Box<dyn Prover<CpuProverComponents>> =
                    Box::new(ProverClient::from_env());
                TxBuilder::new(
                    config.ics26_address,
                    provider,
                    tm_client,
                    prover,
                    sp1_programs,
                )
            }
            SP1Config::Cpu => {
                let prover: Box<dyn Prover<CpuProverComponents>> =
                    Box::new(ProverClient::builder().cpu().build());
                TxBuilder::new(
                    config.ics26_address,
                    provider,
                    tm_client,
                    prover,
                    sp1_programs,
                )
            }
            SP1Config::Cuda => {
                let prover: Box<dyn Prover<CpuProverComponents>> =
                    Box::new(ProverClient::builder().cuda().build());
                TxBuilder::new(
                    config.ics26_address,
                    provider,
                    tm_client,
                    prover,
                    sp1_programs,
                )
            }
            SP1Config::Network {
                network_private_key,
                network_rpc_url,
                private_cluster,
            } => {
                let mut prover_builder = ProverClient::builder().network();
                if let Some(private_key) = network_private_key {
                    prover_builder = prover_builder.private_key(&private_key);
                }
                if let Some(rpc_url) = network_rpc_url {
                    prover_builder = prover_builder.rpc_url(&rpc_url);
                }
                if private_cluster {
                    TxBuilder::new(
                        config.ics26_address,
                        provider,
                        tm_client,
                        prover_builder.build(),
                        sp1_programs,
                    )
                } else {
                    let prover: Box<dyn Prover<CpuProverComponents>> =
                        Box::new(prover_builder.build());
                    TxBuilder::new(
                        config.ics26_address,
                        provider,
                        tm_client,
                        prover,
                        sp1_programs,
                    )
                }
            }
        };

        Self {
            tm_listener,
            eth_listener,
            tx_builder,
        }
    }
}

#[tonic::async_trait]
impl RelayerService for CosmosToEthRelayerModuleService {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Cosmos to Eth...");
        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: self
                    .eth_listener
                    .chain_id()
                    .await
                    .map_err(|e| tonic::Status::from_error(e.into()))?,
                ibc_version: "2".to_string(),
                ibc_contract: self.tx_builder.ics26_router.address().to_string(),
            }),
            source_chain: Some(api::Chain {
                chain_id: self
                    .tm_listener
                    .chain_id()
                    .await
                    .map_err(|e| tonic::Status::from_error(e.into()))?,
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            metadata: self.tx_builder.metadata(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Cosmos to Eth...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());
        let cosmos_txs = inner_req
            .source_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.into()))?;

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
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(cosmos_events = ?cosmos_events, "Fetched Cosmos events.");
        tracing::info!(
            "Fetched {} eureka events from CosmosSDK.",
            cosmos_events.len()
        );

        let eth_events = self
            .eth_listener
            .fetch_tx_events(eth_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(eth_events = ?eth_events, "Fetched EVM events.");
        tracing::info!("Fetched {} eureka events from EVM.", eth_events.len());

        let multicall_tx = self
            .tx_builder
            .relay_events(
                cosmos_events,
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
            tx: multicall_tx,
            address: self.tx_builder.ics26_router.address().to_string(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Cosmos to Eth...");

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
        tracing::info!("Handling update client request for Cosmos to Eth...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .update_client(inner_req.dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Update client request completed.");

        Ok(Response::new(api::UpdateClientResponse {
            tx,
            address: self.tx_builder.ics26_router.address().to_string(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for CosmosToEthRelayerModule {
    fn name(&self) -> &'static str {
        "cosmos_to_eth"
    }

    #[tracing::instrument(skip_all)]
    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config = serde_json::from_value::<CosmosToEthConfig>(config)
            .map_err(|e| anyhow::anyhow!("failed to parse config: {e}"))?;

        tracing::info!("Starting Cosmos to Ethereum relayer server.");
        Ok(Box::new(CosmosToEthRelayerModuleService::new(config).await))
    }
}
