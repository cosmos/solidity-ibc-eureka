//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from Solana.

use prost::Message;
use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use ibc_eureka_relayer_lib::{
    chain::{CosmosSdk, SolanaEureka},
    events::{EurekaEventWithHeight, SolanaEurekaEventWithHeight},
    tx_builder::TxBuilderService,
    utils::cosmos,
};
use ibc_proto_eureka::{
    cosmos::tx::v1beta1::TxBody,
    google::protobuf::Any,
    ibc::{core::client::v1::Height, lightclients::wasm::v1::ConsensusState as WasmConsensusState},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use tendermint_rpc::HttpClient;

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on events from Solana.
#[allow(dead_code)]
pub struct MockTxBuilder {
    /// The Solana RPC client
    pub solana_client: Arc<RpcClient>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
    /// The Solana ICS26 router program ID.
    pub ics26_program_id: Pubkey,
}

impl MockTxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub const fn new(
        solana_client: Arc<RpcClient>,
        tm_client: HttpClient,
        signer_address: String,
        solana_ics26_program_id: Pubkey,
    ) -> Self {
        Self {
            solana_client,
            tm_client,
            signer_address,
            ics26_program_id: solana_ics26_program_id,
        }
    }
}

#[async_trait::async_trait]
impl TxBuilderService<SolanaEureka, CosmosSdk> for MockTxBuilder {
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        src_events: Vec<SolanaEurekaEventWithHeight>,
        dest_events: Vec<EurekaEventWithHeight>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> anyhow::Result<Vec<u8>> {
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

        // NOTE: Convert to eureka event to reuse to recvs/ack msg fn
        let src_events_as_sol_events = src_events
            .into_iter()
            .map(EurekaEventWithHeight::from)
            .collect();

        let (mut recv_msgs, mut ack_msgs) = cosmos::src_events_to_recv_and_ack_msgs(
            src_events_as_sol_events,
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

        for recv in &recv_msgs {
            tracing::info!(?recv, "recv_msgs");
        }

        for ack in &ack_msgs {
            tracing::info!(?ack, "ack_msgs");
        }
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

    #[tracing::instrument(skip_all)]
    async fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        tracing::info!("Creating Solana light client on Cosmos");

        let client_state = b"test".to_vec();
        let consensus_state = WasmConsensusState {
            data: b"test".to_vec(),
        };

        cosmos::cosmos_create_client_tx(
            parameters,
            client_state,
            &consensus_state,
            Height {
                revision_number: 0,
                revision_height: 1,
            },
            self.signer_address.clone(),
        )
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(&self, dst_client_id: String) -> anyhow::Result<Vec<u8>> {
        let consensus_state = WasmConsensusState {
            data: b"test".to_vec(),
        };

        cosmos::cosmos_update_client_tx(
            dst_client_id,
            &consensus_state,
            self.signer_address.clone(),
        )
    }
}
