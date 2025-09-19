//! This is a one-sided relayer module from Solana to a Cosmos SDK chain.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(unused_crate_dependencies, missing_docs)]

pub mod tx_builder;

use std::collections::HashMap;
use std::sync::Arc;

use ibc_eureka_utils::rpc::TendermintRpcExt;
use prost::Message;
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use tendermint::Hash;
use tendermint_rpc::Client;
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
    /// The Solana chain ID.
    pub solana_chain_id: String,
    /// The Solana RPC client (wrapped in Arc since `RpcClient` doesn't implement Clone in 2.0).
    pub solana_client: Arc<RpcClient>,
    /// The target Cosmos tendermint client.
    pub target_tm_client: HttpClient,
    /// The transaction builder from Solana to Cosmos.
    pub tx_builder: tx_builder::TxBuilder,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: solana_sdk::pubkey::Pubkey,
    /// Whether to use mock proofs for testing.
    pub mock: bool,
}

/// The configuration for the Solana to Cosmos relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct SolanaToCosmosConfig {
    /// The Solana chain ID for identification.
    pub solana_chain_id: String,
    /// The Solana RPC URL.
    pub solana_rpc_url: String,
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
        let solana_client = Arc::new(RpcClient::new(config.solana_rpc_url));
        let target_client = HttpClient::from_rpc_url(&config.target_rpc_url);

        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana program ID: {e}"))?;

        let tx_builder = tx_builder::TxBuilder::new(
            Arc::clone(&solana_client),
            target_client.clone(),
            config.signer_address,
            solana_ics26_program_id,
        );

        Ok(Self {
            solana_chain_id: config.solana_chain_id,
            solana_client,
            target_tm_client: target_client,
            tx_builder,
            solana_ics26_program_id,
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

        // Get Cosmos chain ID
        let status = self
            .target_tm_client
            .status()
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to get chain ID: {e}")))?;

        Ok(Response::new(api::InfoResponse {
            target_chain: Some(api::Chain {
                chain_id: status.node_info.network.to_string(),
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            source_chain: Some(api::Chain {
                chain_id: self.solana_chain_id.clone(),
                ibc_version: "2".to_string(),
                ibc_contract: self.solana_ics26_program_id.to_string(),
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

        // Parse Solana transaction signatures
        let src_txs: Vec<Signature> = inner_req
            .source_tx_ids
            .into_iter()
            .map(|tx_id| {
                let sig_str = String::from_utf8(tx_id).map_err(|e| {
                    tonic::Status::invalid_argument(format!("Invalid signature: {e}"))
                })?;
                sig_str
                    .parse::<Signature>()
                    .map_err(|e| tonic::Status::invalid_argument(format!("Invalid signature: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Parse Cosmos transaction hashes for timeouts
        let _target_txs = inner_req
            .timeout_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        // Fetch events from Solana transactions
        let src_events = self
            .tx_builder
            .fetch_solana_events(src_txs)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(solana_src_events = ?src_events, "Fetched source Solana events.");
        tracing::info!(
            "Fetched {} source eureka events from Solana.",
            src_events.len()
        );

        // For timeouts from Cosmos - simplified for now
        let target_events = Vec::new();
        tracing::info!(
            "Fetched {} target eureka events from CosmosSDK.",
            target_events.len()
        );

        // Build the relay transaction
        let mut tx = self
            .tx_builder
            .build_relay_tx(&inner_req.dst_client_id, src_events, target_events)
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
            .build_create_client_tx(&inner_req.parameters)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        Ok(Response::new(api::CreateClientResponse {
            tx: tx.encode_to_vec(),
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
            .build_update_client_tx(inner_req.dst_client_id)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

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
