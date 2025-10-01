//! This is a one-sided relayer module from Solana to a Cosmos SDK chain.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(unused_crate_dependencies, missing_docs)]

pub mod tx_builder;

use std::collections::HashMap;
use std::sync::Arc;

use ibc_eureka_relayer_lib::chain::SolanaEureka;
use ibc_eureka_relayer_lib::events::EurekaEventWithHeight;
use ibc_eureka_relayer_lib::events::SolanaEurekaEvent;
use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_relayer_lib::listener::solana_eureka;
use ibc_eureka_relayer_lib::listener::ChainListenerService;
use ibc_eureka_relayer_lib::service_utils::parse_cosmos_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::parse_solana_tx_hashes;
use ibc_eureka_relayer_lib::service_utils::to_tonic_status;
use ibc_eureka_relayer_lib::utils::solana;
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

        let mut tx = self
            .tx_builder
            .build_relay_tx(&inner_req.dst_client_id, solana_events, cosmos_events)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

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

#[async_trait::async_trait]
impl<P> TxBuilderService<SolanaEureka, CosmosSdk> for TxBuilder {
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        src_events: Vec<EurekaEventWithHeight>,
        dest_events: Vec<EurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<u8>> {
        tracing::info!(
            "Relaying events from Solana to Cosmos for client {}",
            dst_client_id
        );

        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let mut timeout_msgs = cosmos::target_events_to_timeout_msgs(
            dest_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix.as_secs(),
        );

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &self.signer_address,
            now_since_unix.as_secs(),
        );

        tracing::debug!("Timeout messages: #{}", timeout_msgs.len());
        tracing::debug!("Recv messages: #{}", recv_msgs.len());
        tracing::debug!("Ack messages: #{}", ack_msgs.len());

        cosmos::inject_mock_proofs(&mut recv_msgs, &mut ack_msgs, &mut timeout_msgs);

        let all_msgs = timeout_msgs
            .into_iter()
            .map(|m| Any::from_msg(&m))
            .chain(recv_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .chain(ack_msgs.into_iter().map(|m| Any::from_msg(&m)))
            .collect::<Result<Vec<_>, _>>()?;

        let tx_body = TxBody {
            messages: all_msgs,
            ..Default::default()
        };
        Ok(tx_body.encode_to_vec())
    }

    // TODO: Update once real solana light client is available
    #[tracing::instrument(skip_all)]
    async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        parameters
            .keys()
            .find(|k| k.as_str() != CHECKSUM_HEX)
            .map_or(Ok(()), |param| {
                Err(anyhow::anyhow!(
                    "Unexpected parameter: `{param}`, only `{CHECKSUM_HEX}` is allowed"
                ))
            })?;

        let client_state = WasmClientState {
            data: b"test".to_vec(),
            checksum: hex::decode(
                parameters
                    .get(CHECKSUM_HEX)
                    .ok_or_else(|| anyhow::anyhow!("Missing `{CHECKSUM_HEX}` parameter"))?,
            )?,
            latest_height: Some(Height {
                revision_number: 0,
                revision_height: 1,
            }),
        };
        let consensus_state = WasmConsensusState {
            data: b"test".to_vec(),
        };

        let msg = MsgCreateClient {
            client_state: Some(Any::from_msg(&client_state)?),
            consensus_state: Some(Any::from_msg(&consensus_state)?),
            signer: self.signer_address.clone(),
        };

        Ok(TxBody {
            messages: vec![Any::from_msg(&msg)?],
            ..Default::default()
        }
        .encode_to_vec())
    }

    // TODO: Update once real solana light client is available
    #[tracing::instrument(skip_all)]
    async fn update_client(&self, dst_client_id: String) -> Result<Vec<u8>> {
        tracing::info!(
            "Generating tx to update mock light client: {}",
            dst_client_id
        );

        let consensus_state = WasmConsensusState {
            data: b"test".to_vec(),
        };
        let msg = MsgUpdateClient {
            client_id: dst_client_id,
            client_message: Some(Any::from_msg(&consensus_state)?),
            signer: self.signer_address.clone(),
        };

        Ok(TxBody {
            messages: vec![Any::from_msg(&msg)?],
            ..Default::default()
        }
        .encode_to_vec())
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
