//! The `ChainSubmitter` submits txs to [`EthEureka`] based on events from [`CosmosSdk`].

#![allow(unused_imports)]

use std::str::FromStr;

use alloy::{
    primitives::{Address, TxHash},
    providers::Provider,
    transports::Transport,
};
use anyhow::Result;
use futures::future;
use ibc_core_host_types::identifiers::ChainId;
use ibc_eureka_solidity_types::{
    ics02::client::clientInstance,
    ics26::{
        router::{ackPacketCall, recvPacketCall, routerCalls, routerInstance, timeoutPacketCall},
        IICS02ClientMsgs::Height,
        IICS26RouterMsgs::{MsgAckPacket, MsgRecvPacket, MsgTimeoutPacket},
    },
};
use itertools::Itertools;
use sp1_ics07_tendermint_prover::{
    programs::UpdateClientAndMembershipProgram,
    prover::{SP1ICS07TendermintProver, SupportedProofType},
};
use sp1_ics07_tendermint_solidity::{sp1_ics07_tendermint, IICS07TendermintMsgs::ClientState};
use sp1_ics07_tendermint_utils::{merkle::convert_tm_to_ics_merkle_proof, rpc::TendermintRpcExt};
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
    ics26_router: routerInstance<T, P>,
    /// The HTTP client for the Cosmos SDK.
    tm_client: HttpClient,
    /// The proof type to use for [`SP1ICS07TendermintProver`].
    proof_type: SupportedProofType,
}

impl<T: Transport + Clone, P: Provider<T> + Clone> ChainSubmitter<T, P> {
    /// Create a new `ChainListenerService` instance.
    pub const fn new(
        ics26_address: Address,
        provider: P,
        tm_client: HttpClient,
        proof_type: SupportedProofType,
    ) -> Self {
        Self {
            ics26_router: routerInstance::new(ics26_address, provider),
            tm_client,
            proof_type,
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
    async fn submit_events(
        &self,
        src_events: Vec<EurekaEvent>,
        dest_events: Vec<EurekaEvent>,
    ) -> Result<TxHash> {
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

        let timeout_msgs = src_events.into_iter().filter_map(|e| match e {
            EurekaEvent::SendPacket(se) => {
                if now >= se.packet.timeoutTimestamp {
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
                if se.packet.timeoutTimestamp > now {
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
                Some(routerCalls::ackPacket(ackPacketCall {
                    msg_: MsgAckPacket {
                        packet: we.packet,
                        acknowledgement: we.acknowledgements[0].clone(), // TODO: handle multiple acks
                        proofHeight: latest_height.clone(),
                        proofAcked: b"".into(),
                    },
                }))
            }
            _ => None,
        });

        let _calls_by_channel = timeout_msgs
            .chain(recv_and_ack_msgs)
            .into_group_map_by(|call| match &call {
                routerCalls::timeoutPacket(c) => c.msg_.packet.sourceChannel.clone(),
                routerCalls::recvPacket(c) => c.msg_.packet.destChannel.clone(),
                routerCalls::ackPacket(c) => c.msg_.packet.sourceChannel.clone(),
                _ => unreachable!(),
            });

        // TODO: Filter already submitted packets

        //let events_by_channel = source_events_by_channel
        //    .chain(dest_events_by_channel)
        //    .into_group_map();
        //
        //for channel_id in events_by_channel.keys() {
        //    let client_state = self.client_state(channel_id.clone()).await?;
        //}

        //let ibc_paths = timeout_msg
        //    .map(|msg| msg.packet.receipt_commitment_path())
        //    .chain(
        //        recv_msgs
        //            .iter()
        //            .map(|msg| msg.packet.commitment_path())
        //            .chain(ack_msgs.iter().map(|msg| msg.packet.ack_commitment_path())),
        //    )
        //    .map(|path| vec![b"ibc".into(), path]);

        //let kv_proofs: Vec<(Vec<Vec<u8>>, Vec<u8>, _)> =
        //    future::try_join_all(ibc_paths.into_iter().map(|path| async {
        //        let res = self
        //            .tm_client
        //            .abci_query(
        //                Some(format!("store/{}/key", std::str::from_utf8(&path[0])?)),
        //                path[1].as_slice(),
        //                // Proof height should be the block before the target block.
        //                Some((revision_height - 1).into()),
        //                true,
        //            )
        //            .await?;
        //
        //        if u32::try_from(res.height.value())? + 1 != revision_height {
        //            anyhow::bail!("Proof height mismatch");
        //        }
        //
        //        if res.key.as_slice() != path[1].as_slice() {
        //            anyhow::bail!("Key mismatch");
        //        }
        //        let vm_proof = convert_tm_to_ics_merkle_proof(&res.proof.unwrap())?;
        //        if vm_proof.proofs.is_empty() {
        //            anyhow::bail!("Empty proof");
        //        }
        //
        //        anyhow::Ok((path, res.value, vm_proof))
        //    }))
        //    .await?;
        //
        //let sp1_uc_and_mem_prover =
        //    SP1ICS07TendermintProver::<UpdateClientAndMembershipProgram>::new(self.proof_type);

        todo!()
    }
}
