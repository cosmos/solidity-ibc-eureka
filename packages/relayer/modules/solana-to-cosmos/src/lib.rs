//! This is a one-sided relayer module from Solana to a Cosmos SDK chain.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(unused_crate_dependencies, missing_docs)]

pub mod tx_builder;

use std::collections::HashMap;
use std::sync::Arc;

use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_relayer_lib::listener::solana_eureka;
use ibc_eureka_relayer_lib::listener::ChainListenerService;
use ibc_eureka_relayer_lib::service_utils::parse_cosmos_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::parse_solana_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::to_tonic_status;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use prost::Message;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};
use ibc_eureka_relayer_lib::utils::cosmos;

/// The `SolanaToCosmosRelayerModule` struct defines the Solana to Cosmos relayer module.
#[derive(Clone, Copy, Debug)]
pub struct SolanaToCosmosRelayerModule;

/// The `SolanaToCosmosRelayerModuleService` defines the relayer service from Solana to Cosmos.
#[allow(dead_code)]
struct SolanaToCosmosRelayerModuleService {
    /// The souce chain listener for Solana.
    pub src_listener: solana_eureka::ChainListener,
    /// The target chain listener for Cosmos SDK.
    pub target_listener: cosmos_sdk::ChainListener,
    /// The transaction builder from Solana to Cosmos.
    pub tx_builder: tx_builder::TxBuilder,
    /// Whether to use mock proofs for testing.
    pub mock: bool,
}

/// The configuration for the Solana to Cosmos relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SolanaToCosmosConfig {
    /// The Solana chain ID for identification.
    pub solana_chain_id: String,
    /// The Solana RPC URL.
    pub src_rpc_url: String,
    /// The target tendermint RPC URL.
    pub target_rpc_url: String,
    /// The address of the submitter on Cosmos.
    /// Required since cosmos messages require a signer address.
    pub signer_address: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
    /// Whether to use mock proofs for testing.
    pub mock: bool,
}

impl SolanaToCosmosRelayerModuleService {
    fn new(config: SolanaToCosmosConfig) -> anyhow::Result<Self> {
        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana program ID: {e}"))?;

        let solana_ics07_program_id = config
            .solana_ics07_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS07 program ID: {}", e))?;

        let src_listener = solana_eureka::ChainListener::new(
            config.src_rpc_url.clone(),
            solana_ics26_program_id,
            solana_ics07_program_id,
        );

        let target_listener =
            cosmos_sdk::ChainListener::new(HttpClient::from_rpc_url(&config.target_rpc_url));

        let tx_builder = tx_builder::TxBuilder::new(
            Arc::clone(src_listener.client()),
            target_listener.client().clone(),
            config.signer_address,
            solana_ics26_program_id,
        );

        Ok(Self {
            src_listener,
            target_listener,
            tx_builder,
            mock: config.mock,
        })
    }

    /// Injects mock proofs into IBC messages in the transaction for testing purposes.
    fn inject_mock_proofs_into_tx(
        tx: &mut ibc_proto_eureka::cosmos::tx::v1beta1::TxBody,
    ) -> anyhow::Result<()> {
        use ibc_proto_eureka::ibc::core::channel::v2::{
            MsgAcknowledgement, MsgRecvPacket, MsgTimeout,
        };

        for any_msg in &mut tx.messages {
            match any_msg.type_url.as_str() {
                url if url.contains("MsgRecvPacket") => {
                    let msg = MsgRecvPacket::decode(any_msg.value.as_slice())?;
                    let mut msgs = [msg];
                    cosmos::inject_mock_proofs(&mut msgs, &mut [], &mut []);
                    any_msg.value = msgs[0].encode_to_vec();
                }
                url if url.contains("MsgAcknowledgement") => {
                    let msg = MsgAcknowledgement::decode(any_msg.value.as_slice())?;
                    let mut msgs = [msg];
                    cosmos::inject_mock_proofs(&mut [], &mut msgs, &mut []);
                    any_msg.value = msgs[0].encode_to_vec();
                }
                url if url.contains("MsgTimeout") => {
                    let msg = MsgTimeout::decode(any_msg.value.as_slice())?;
                    let mut msgs = [msg];
                    cosmos::inject_mock_proofs(&mut [], &mut [], &mut msgs);
                    any_msg.value = msgs[0].encode_to_vec();
                }
                _ => {} // Skip messages we don't care about
            }
        }

        Ok(())
    }
}

#[tonic::async_trait]
impl RelayerService for SolanaToCosmosRelayerModuleService {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Solana to Cosmos...");

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
                chain_id: "solana".to_string(), // Solana doesn't have chain IDs like Cosmos
                ibc_version: "2".to_string(),
                ibc_contract: self.src_listener.ics26_router_program_id().to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Solana to Cosmos...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        let solana_tx_hashes = parse_solana_tx_hashes(inner_req.source_tx_ids)?;

        let cosmos_txs = parse_cosmos_tx_hashes(inner_req.timeout_tx_ids)?;

        let solana_events = self
            .src_listener
            .fetch_tx_events(solana_tx_hashes)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(?solana_events, "Fetched source Solana events.");
        tracing::info!(
            "Fetched {} source eureka events from Solana.",
            solana_events.len()
        );

        let cosmos_events = self
            .target_listener
            .fetch_tx_events(cosmos_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(cosmos_events = ?cosmos_events, "Fetched Cosmos events.");
        tracing::info!(
            "Fetched {} eureka events from CosmosSDK.",
            cosmos_events.len()
        );

        // Build the relay transaction
        let mut tx = self
            .tx_builder
            .build_relay_tx(&inner_req.dst_client_id, solana_events, cosmos_events)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        // Inject mock proofs if in mock mode
        if self.mock {
            tracing::info!("=== INJECTING MOCK PROOFS ===");
            tracing::info!(
                "Mock mode enabled, injecting mock proofs into {} messages",
                tx.messages.len()
            );
            Self::inject_mock_proofs_into_tx(&mut tx).map_err(|e| {
                tonic::Status::internal(format!("Failed to inject mock proofs: {e}"))
            })?;
            tracing::info!("Successfully injected mock proofs into transaction messages");
        } else {
            tracing::warn!("Mock mode disabled - no proofs will be injected!");
        }

        tracing::info!(
            "Built {} messages for Solana to Cosmos relay.",
            tx.messages.len()
        );

        Ok(Response::new(api::RelayByTxResponse {
            tx: tx.encode_to_vec(),
            address: String::new(),
            chunked_txs: vec![],
            chunked_metadata: None,
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Solana to Cosmos...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .create_client(&inner_req.parameters)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

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
        tracing::info!("Handling update client request for Solana to Cosmos...");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .update_client(inner_req.dst_client_id)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Update client request completed.");

        Ok(Response::new(api::UpdateClientResponse {
            tx: tx.encode_to_vec(),
            address: String::new(),
            chunked_metadata: None,
            chunked_txs: vec![],
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for SolanaToCosmosRelayerModule {
    fn name(&self) -> &'static str {
        "solana_to_cosmos"
    }

    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config: SolanaToCosmosConfig = serde_json::from_value(config)?;
        let service = SolanaToCosmosRelayerModuleService::new(config)?;
        Ok(Box::new(service))
    }
}
