//! The `ChainSubmitter` submits txs to [`EthEureka`] based on events from [`CosmosSdk`].

use std::{env, str::FromStr};

use alloy::{
    primitives::Address,
    providers::Provider,
    sol_types::{SolCall, SolValue},
    transports::Transport,
};
use anyhow::Result;
use futures::future;
use ibc_core_host_types::identifiers::ChainId;
use ibc_eureka_solidity_types::{
    ics02::client::clientInstance,
    ics26::{
        router::{
            ackPacketCall, multicallCall, recvPacketCall, routerCalls, routerInstance,
            timeoutPacketCall,
        },
        IICS02ClientMsgs::Height,
        IICS26RouterMsgs::{MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket},
    },
};
// Re-export the `SupportedProofType` enum.
pub use sp1_ics07_tendermint_prover::prover::SupportedProofType;
use sp1_ics07_tendermint_prover::{
    programs::UpdateClientAndMembershipProgram, prover::SP1ICS07TendermintProver,
};

use sp1_ics07_tendermint_solidity::{
    sp1_ics07_tendermint,
    IICS07TendermintMsgs::ClientState,
    IMembershipMsgs::{MembershipProof, SP1MembershipAndUpdateClientProof},
    ISP1Msgs::SP1Proof,
};
use sp1_ics07_tendermint_utils::{
    light_block::LightBlockExt, merkle::convert_tm_to_ics_merkle_proof, rpc::TendermintRpcExt,
};
use sp1_sdk::HashableKey;
use tendermint_rpc::{Client, HttpClient};

use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEvent,
};

use super::r#trait::ChainSubmitterService;

/// The `ChainSubmitter` submits txs to [`EthEureka`] based on events from [`CosmosSdk`].
#[allow(dead_code)]
pub struct ChainSubmitter<T: Transport + Clone, P: Provider<T> + Clone> {
    /// The IBC Eureka router instance.
    pub ics26_router: routerInstance<T, P>,
    /// The HTTP client for the Cosmos SDK.
    pub tm_client: HttpClient,
    /// The proof type to use for [`SP1ICS07TendermintProver`].
    pub proof_type: SupportedProofType,
    /// The SP1 private key for the prover network
    /// Uses the local prover if not set
    pub sp1_private_key: Option<String>,
}

impl<T: Transport + Clone, P: Provider<T> + Clone> ChainSubmitter<T, P> {
    /// Create a new `ChainListenerService` instance.
    pub fn new(
        ics26_address: Address,
        provider: P,
        tm_client: HttpClient,
        proof_type: SupportedProofType,
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
            proof_type,
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
}

#[async_trait::async_trait]
impl<T, P> ChainSubmitterService<EthEureka, CosmosSdk> for ChainSubmitter<T, P>
where
    T: Transport + Clone,
    P: Provider<T> + Clone,
{
    #[allow(clippy::too_many_lines)]
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

        let filter_channel = target_channel_id.clone();
        let timeout_msgs = src_events.into_iter().filter_map(|e| match e {
            EurekaEvent::SendPacket(se) => {
                if now >= se.packet.timeoutTimestamp && se.packet.sourceChannel == filter_channel {
                    Some(routerCalls::timeoutPacket(timeoutPacketCall {
                        msg_: MsgTimeoutPacket {
                            packet: se.packet,
                            proofHeight: latest_height.clone(),
                            proofTimeout: b"".into(),
                        },
                    }))
                } else {
                    None
                }
            }
            _ => None,
        });

        let recv_and_ack_msgs = dest_events.into_iter().filter_map(|e| match e {
            EurekaEvent::SendPacket(se) => {
                if se.packet.timeoutTimestamp > now && se.packet.destChannel == filter_channel {
                    Some(routerCalls::recvPacket(recvPacketCall {
                        msg_: MsgRecvPacket {
                            packet: se.packet,
                            proofHeight: latest_height.clone(),
                            proofCommitment: b"".into(),
                        },
                    }))
                } else {
                    None
                }
            }
            EurekaEvent::WriteAcknowledgement(we) => {
                if we.packet.sourceChannel == filter_channel {
                    Some(routerCalls::ackPacket(ackPacketCall {
                        msg_: MsgAckPacket {
                            packet: we.packet,
                            acknowledgement: we.acknowledgements[0].clone(), // TODO: handle multiple acks
                            proofHeight: latest_height.clone(),
                            proofAcked: b"".into(),
                        },
                    }))
                } else {
                    None
                }
            }
            _ => None,
        });

        let mut all_msgs = timeout_msgs.chain(recv_and_ack_msgs).collect::<Vec<_>>();

        // TODO: Filter already submitted packets

        let ibc_paths = all_msgs
            .iter()
            .map(|msg| match msg {
                routerCalls::timeoutPacket(call) => call.msg_.packet.receipt_commitment_path(),
                routerCalls::recvPacket(call) => call.msg_.packet.commitment_path(),
                routerCalls::ackPacket(call) => call.msg_.packet.ack_commitment_path(),
                _ => unreachable!(),
            })
            .map(|path| vec![b"ibc".into(), path]);

        let kv_proofs: Vec<(Vec<Vec<u8>>, Vec<u8>, _)> =
            future::try_join_all(ibc_paths.into_iter().map(|path| async {
                let res = self
                    .tm_client
                    .abci_query(
                        Some(format!("store/{}/key", std::str::from_utf8(&path[0])?)),
                        path[1].as_slice(),
                        // Proof height should be the block before the target block.
                        Some((revision_height - 1).into()),
                        true,
                    )
                    .await?;

                if u32::try_from(res.height.value())? + 1 != revision_height {
                    anyhow::bail!("Proof height mismatch");
                }

                if res.key.as_slice() != path[1].as_slice() {
                    anyhow::bail!("Key mismatch");
                }
                let vm_proof = convert_tm_to_ics_merkle_proof(&res.proof.unwrap())?;
                if vm_proof.proofs.is_empty() {
                    anyhow::bail!("Empty proof");
                }

                anyhow::Ok((path, res.value, vm_proof))
            }))
            .await?;

        let client_state = self.client_state(target_channel_id).await?;
        let trusted_light_block = self
            .tm_client
            .get_light_block(Some(client_state.latestHeight.revisionHeight))
            .await?;

        // Get the proposed header from the target light block.
        let proposed_header = latest_light_block.into_header(&trusted_light_block);

        let uc_and_mem_prover =
            SP1ICS07TendermintProver::<UpdateClientAndMembershipProgram>::new(self.proof_type);

        let uc_and_mem_proof = uc_and_mem_prover.generate_proof(
            &client_state,
            &trusted_light_block.to_consensus_state().into(),
            &proposed_header,
            now,
            kv_proofs,
        );

        let sp1_proof = MembershipProof::from(SP1MembershipAndUpdateClientProof {
            sp1Proof: SP1Proof::new(
                &uc_and_mem_prover.vkey.bytes32(),
                uc_and_mem_proof.bytes(),
                uc_and_mem_proof.public_values.to_vec(),
            ),
        });

        // inject proof
        match all_msgs.first_mut() {
            Some(routerCalls::timeoutPacket(ref mut call)) => {
                *call.msg_.proofTimeout = sp1_proof.abi_encode().into();
            }
            Some(routerCalls::recvPacket(ref mut call)) => {
                *call.msg_.proofCommitment = sp1_proof.abi_encode().into();
            }
            Some(routerCalls::ackPacket(ref mut call)) => {
                *call.msg_.proofAcked = sp1_proof.abi_encode().into();
            }
            _ => unreachable!(),
        }

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
