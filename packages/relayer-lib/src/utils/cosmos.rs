//! Relayer utilities for `CosmosSDK` chains.

use anyhow::Result;
use futures::future;
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;
use ibc_proto_eureka::{
    ibc::core::{
        channel::v2::{Acknowledgement, MsgAcknowledgement, MsgRecvPacket, MsgTimeout},
        client::v1::Height,
    },
    Protobuf,
};
use sp1_ics07_tendermint_utils::rpc::TendermintRpcExt;
use tendermint_rpc::HttpClient;

use crate::events::EurekaEvent;

/// Converts a list of [`EurekaEvent`]s to a list of [`MsgTimeout`]s.
pub fn target_events_to_timeout_msgs(
    target_events: Vec<EurekaEvent>,
    target_channel_id: &str,
    target_height: &Height,
    signer_address: &str,
    now: u64,
) -> Vec<MsgTimeout> {
    target_events
        .into_iter()
        .filter_map(|e| match e {
            EurekaEvent::SendPacket(se) => {
                if now >= se.packet.timeoutTimestamp && se.packet.sourceChannel == target_channel_id
                {
                    Some(MsgTimeout {
                        packet: Some(se.packet.into()),
                        proof_height: Some(*target_height),
                        proof_unreceived: vec![],
                        signer: signer_address.to_string(),
                    })
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect()
}

/// Converts a list of [`EurekaEvent`]s to a list of [`MsgRecvPacket`]s and
/// [`MsgAcknowledgement`]s.
/// # Errors
/// Returns an error if proof cannot be generated, or membership value is empty for a packet.
pub fn src_events_to_recv_and_ack_msgs(
    src_events: Vec<EurekaEvent>,
    target_channel_id: &str,
    target_height: &Height,
    signer_address: &str,
    now: u64,
) -> (Vec<MsgRecvPacket>, Vec<MsgAcknowledgement>) {
    let (src_send_events, src_ack_events): (Vec<_>, Vec<_>) = src_events
        .into_iter()
        .filter(|e| match e {
            EurekaEvent::SendPacket(se) => {
                se.packet.timeoutTimestamp > now && se.packet.destChannel == target_channel_id
            }
            EurekaEvent::WriteAcknowledgement(we) => we.packet.sourceChannel == target_channel_id,
            EurekaEvent::RecvPacket(_) => false,
        })
        .partition(|e| match e {
            EurekaEvent::SendPacket(_) => true,
            EurekaEvent::WriteAcknowledgement(_) => false,
            EurekaEvent::RecvPacket(_) => unreachable!(),
        });

    let recv_msgs = src_send_events
        .into_iter()
        .map(|e| match e {
            EurekaEvent::SendPacket(se) => MsgRecvPacket {
                packet: Some(se.packet.into()),
                proof_height: Some(*target_height),
                proof_commitment: vec![],
                signer: signer_address.to_string(),
            },
            _ => unreachable!(),
        })
        .collect::<Vec<MsgRecvPacket>>();

    let ack_msgs = src_ack_events
        .into_iter()
        .map(|e| match e {
            EurekaEvent::WriteAcknowledgement(we) => MsgAcknowledgement {
                packet: Some(we.packet.into()),
                acknowledgement: Some(Acknowledgement {
                    app_acknowledgements: we.acknowledgements.into_iter().map(Into::into).collect(),
                }),
                proof_height: Some(*target_height),
                proof_acked: vec![],
                signer: signer_address.to_string(),
            },
            _ => unreachable!(),
        })
        .collect::<Vec<MsgAcknowledgement>>();

    (recv_msgs, ack_msgs)
}

pub async fn inject_tendermint_proofs(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
    source_tm_client: &HttpClient,
    target_height: &Height,
) -> Result<()> {
    future::try_join_all(recv_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().try_into()?;
        let commitment_path = packet.commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(
                &[b"ibc".to_vec(), commitment_path],
                target_height.revision_height.try_into().unwrap(),
            )
            .await?;
        if value.is_empty() {
            anyhow::bail!("Membership value is empty")
        }

        msg.proof_commitment = proof.encode_vec();
        msg.proof_height = Some(*target_height);
        anyhow::Ok(())
    }))
    .await?;

    future::try_join_all(ack_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().try_into()?;
        let ack_path = packet.ack_commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(
                &[b"ibc".to_vec(), ack_path],
                target_height.revision_height.try_into().unwrap(),
            )
            .await?;
        if value.is_empty() {
            anyhow::bail!("Membership value is empty")
        }

        msg.proof_acked = proof.encode_vec();
        msg.proof_height = Some(*target_height);
        anyhow::Ok(())
    }))
    .await?;

    future::try_join_all(timeout_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().try_into()?;
        let receipt_path = packet.receipt_commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(
                &[b"ibc".to_vec(), receipt_path],
                target_height.revision_height.try_into().unwrap(),
            )
            .await?;

        if !value.is_empty() {
            anyhow::bail!("Non-Membership value is empty")
        }
        msg.proof_unreceived = proof.encode_vec();
        msg.proof_height = Some(*target_height);
        anyhow::Ok(())
    }))
    .await?;

    Ok(())
}
