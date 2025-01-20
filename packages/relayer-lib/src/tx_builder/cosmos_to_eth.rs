//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Ethereum chain from events received from the Cosmos SDK chain.

use std::{env, str::FromStr};

use alloy::{primitives::Address, providers::Provider, sol_types::SolCall, transports::Transport};
use anyhow::Result;
use ibc_core_host_types::identifiers::ChainId;
use ibc_eureka_solidity_types::{
    ics02::client::clientInstance,
    ics26::{
        router::{multicallCall, routerCalls, routerInstance},
        IICS02ClientMsgs::Height,
    },
    sp1_ics07::{sp1_ics07_tendermint, IICS07TendermintMsgs::ClientState},
};
// Re-export the `SupportedProofType` enum.
pub use sp1_ics07_tendermint_prover::prover::SupportedProofType;

use sp1_ics07_tendermint_utils::rpc::TendermintRpcExt;
use sp1_sdk::{Prover, ProverClient};
use tendermint_rpc::HttpClient;

use sp1_prover::components::CpuProverComponents;

use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEvent,
    utils::eth_eureka::{self, inject_sp1_proof},
};

use super::r#trait::TxBuilderService;

/// The `TxBuilder` produces txs to [`EthEureka`] based on events from [`CosmosSdk`].
#[allow(dead_code)]
pub struct TxBuilder<T: Transport + Clone, P: Provider<T> + Clone> {
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<T, P>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
    /// The SP1 private key for the prover network
    /// Uses the local prover if not set
    pub sp1_private_key: Option<String>,
}

impl<T: Transport + Clone, P: Provider<T> + Clone> TxBuilder<T, P> {
    /// Create a new [`TxBuilder`] instance.
    pub fn new(
        ics26_address: Address,
        provider: P,
        tm_client: HttpClient,
        sp1_private_key: Option<String>,
    ) -> Self {
        if let Some(sp1_private_key) = &sp1_private_key {
            env::set_var("SP1_PROVER", "network");
            env::set_var("SP1_PRIVATE_KEY", sp1_private_key);
        } else {
            env::set_var("SP1_PROVER", "local");
        }

        Self {
            ics26_router: routerInstance::new(ics26_address, provider),
            tm_client,
            sp1_private_key,
        }
    }

    /// Get the client state for a given client ID.
    /// # Errors
    /// Returns an error if the client state cannot be retrieved.
    pub async fn client_state(&self, client_id: String) -> Result<ClientState> {
        let ics02_address = self.ics26_router.ICS02_CLIENT().call().await?._0;
        let ics07_address =
            clientInstance::new(ics02_address, self.ics26_router.provider().clone())
                .getClient(client_id)
                .call()
                .await?
                ._0;

        Ok(
            sp1_ics07_tendermint::new(ics07_address, self.ics26_router.provider().clone())
                .getClientState()
                .call()
                .await?
                ._0,
        )
    }

    /// Get the prover to use for generating SP1 proofs.
    // TODO: Support other prover types
    #[allow(clippy::option_if_let_else)]
    pub fn sp1_prover(&self) -> Box<dyn Prover<CpuProverComponents>> {
        if let Some(sp1_private_key) = &self.sp1_private_key {
            Box::new(
                ProverClient::builder()
                    .network()
                    .private_key(sp1_private_key)
                    .build(),
            )
        } else {
            Box::new(ProverClient::builder().cpu().build())
        }
    }
}

#[async_trait::async_trait]
impl<T, P> TxBuilderService<EthEureka, CosmosSdk> for TxBuilder<T, P>
where
    T: Transport + Clone,
    P: Provider<T> + Clone,
{
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        src_events: Vec<EurekaEvent>,
        dest_events: Vec<EurekaEvent>,
        target_channel_id: String,
    ) -> Result<Vec<u8>> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let latest_light_block = self.tm_client.get_light_block(None).await?;
        let revision_height = u32::try_from(latest_light_block.height().value())?;
        let chain_id =
            ChainId::from_str(latest_light_block.signed_header.header.chain_id.as_str())?;
        let latest_height = Height {
            revisionNumber: chain_id.revision_number().try_into()?,
            revisionHeight: revision_height,
        };

        let timeout_msgs = eth_eureka::target_events_to_timeout_msgs(
            dest_events,
            &target_channel_id,
            &latest_height,
            now,
        );

        let recv_and_ack_msgs = eth_eureka::src_events_to_recv_and_ack_msgs(
            src_events,
            &target_channel_id,
            &latest_height,
            now,
        );

        let mut all_msgs = timeout_msgs
            .into_iter()
            .chain(recv_and_ack_msgs.into_iter())
            .collect::<Vec<_>>();
        if all_msgs.is_empty() {
            anyhow::bail!("No messages to relay to Ethereum");
        }

        tracing::debug!("Messages to be relayed to Ethereum: {:?}", all_msgs);

        let client_state = self.client_state(target_channel_id).await?;

        inject_sp1_proof(
            self.sp1_prover(),
            &mut all_msgs,
            &self.tm_client,
            latest_light_block,
            client_state,
            now,
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
