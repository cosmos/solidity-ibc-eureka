//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Ethereum chain from events received from the Cosmos SDK chain.

use std::str::FromStr;

use alloy::{primitives::Address, providers::Provider, sol_types::SolCall};
use anyhow::Result;
use ibc_core_host_types::identifiers::ChainId;
use ibc_eureka_solidity_types::{
    ics26::{
        router::{multicallCall, routerCalls, routerInstance},
        IICS02ClientMsgs::Height,
    },
    msgs::IICS07TendermintMsgs::ClientState,
    sp1_ics07::sp1_ics07_tendermint,
};
use ibc_eureka_utils::rpc::TendermintRpcExt;
use sp1_ics07_tendermint_prover::{programs::UpdateClientAndMembershipProgram, prover::Sp1Prover};
use tendermint_rpc::HttpClient;

use sp1_prover::components::SP1ProverComponents;

use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEventWithHeight,
    utils::eth_eureka::{self, inject_sp1_proof},
};

use super::r#trait::TxBuilderService;

/// The `TxBuilder` produces txs to [`EthEureka`] based on events from [`CosmosSdk`].
#[allow(dead_code)]
pub struct TxBuilder<P, C>
where
    P: Provider + Clone,
    C: SP1ProverComponents,
{
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<(), P>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
    /// SP1 prover for generating proofs.
    pub sp1_prover: Sp1Prover<C>,
    /// The SP1 program for updating the client and verifying membership.
    pub uc_and_membership_program: UpdateClientAndMembershipProgram,
}

impl<P, C> TxBuilder<P, C>
where
    P: Provider + Clone,
    C: SP1ProverComponents,
{
    /// Create a new [`TxBuilder`] instance.
    pub fn new(
        ics26_address: Address,
        provider: P,
        tm_client: HttpClient,
        sp1_prover: impl Into<Sp1Prover<C>>,
        uc_and_membership_program: UpdateClientAndMembershipProgram,
    ) -> Self {
        Self {
            ics26_router: routerInstance::new(ics26_address, provider),
            tm_client,
            sp1_prover: sp1_prover.into(),
            uc_and_membership_program,
        }
    }

    /// Get the client state for a given client ID.
    /// # Errors
    /// Returns an error if the client state cannot be retrieved.
    pub async fn client_state(&self, client_id: String) -> Result<ClientState> {
        let ics07_address = self.ics26_router.getClient(client_id).call().await?._0;
        Ok(
            sp1_ics07_tendermint::new(ics07_address, self.ics26_router.provider().clone())
                .clientState()
                .call()
                .await?
                .into(),
        )
    }
}

#[async_trait::async_trait]
impl<P, C> TxBuilderService<EthEureka, CosmosSdk> for TxBuilder<P, C>
where
    P: Provider + Clone,
    C: SP1ProverComponents,
{
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
        let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;

        let latest_light_block = self.tm_client.get_light_block(None).await?;
        let revision_height = latest_light_block.height().value();
        let chain_id =
            ChainId::from_str(latest_light_block.signed_header.header.chain_id.as_str())?;
        let latest_height = Height {
            revisionNumber: chain_id.revision_number(),
            revisionHeight: revision_height,
        };

        let timeout_msgs = eth_eureka::target_events_to_timeout_msgs(
            dest_events,
            &src_client_id,
            &dst_client_id,
            &dst_packet_seqs,
            &latest_height,
            now_since_unix.as_secs(),
        );

        let recv_and_ack_msgs = eth_eureka::src_events_to_recv_and_ack_msgs(
            src_events,
            &src_client_id,
            &dst_client_id,
            &src_packet_seqs,
            &dst_packet_seqs,
            &latest_height,
            now_since_unix.as_secs(),
        );

        let mut all_msgs = timeout_msgs
            .into_iter()
            .chain(recv_and_ack_msgs.into_iter())
            .collect::<Vec<_>>();
        if all_msgs.is_empty() {
            anyhow::bail!("No messages to relay to Ethereum");
        }

        tracing::debug!("Messages to be relayed to Ethereum: {:?}", all_msgs);

        let client_state = self.client_state(dst_client_id).await?;

        inject_sp1_proof(
            &self.sp1_prover,
            &self.uc_and_membership_program,
            &mut all_msgs,
            &self.tm_client,
            latest_light_block,
            client_state,
            now_since_unix.as_nanos(),
        )
        .await?;

        let calls = all_msgs.into_iter().map(|msg| match msg {
            routerCalls::timeoutPacket(call) => call.abi_encode(),
            routerCalls::recvPacket(call) => call.abi_encode(),
            routerCalls::ackPacket(call) => call.abi_encode(),
            _ => unreachable!(),
        });

        let multicall_tx = multicallCall {
            data: calls.map(Into::into).collect(),
        };

        Ok(multicall_tx.abi_encode())
    }
}
