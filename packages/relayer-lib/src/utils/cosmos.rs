//! Relayer utilities for `CosmosSDK` chains.

use alloy_primitives::U256;
use alloy_provider::Provider;
use alloy_transport::Transport;
use anyhow::Result;
use ethereum_apis::eth_api::client::EthApiClient;
use ethereum_light_client::membership::ibc_commitment_key_v2;
use ethereum_types::execution::storage_proof::StorageProof;
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

#[allow(clippy::too_many_arguments)]
pub async fn inject_ethereum_proofs<T: Transport + Clone, P: Provider<T> + Clone>(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
    eth_client: &EthApiClient<T, P>,
    ibc_contrct_address: &str,
    ibc_contract_slot: U256,
    target_block_number: u64,
    target_slot: u64,
) -> Result<()> {
    let target_height = Height {
        revision_number: 0,
        revision_height: target_slot,
    };
    future::try_join_all(recv_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().try_into()?;
        let commitment_path = packet.commitment_path();
        let storage_proof = get_commitment_proof(
            eth_client,
            ibc_contrct_address,
            target_block_number,
            commitment_path,
            ibc_contract_slot,
        )
        .await?;
        if storage_proof.value.is_zero() {
            anyhow::bail!("Membership value is empty")
        }

        msg.proof_commitment = serde_json::to_vec(&storage_proof)?;
        msg.proof_height = Some(target_height);
        anyhow::Ok(())
    }))
    .await?;

    future::try_join_all(ack_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().try_into()?;
        let ack_path = packet.ack_commitment_path();
        let storage_proof = get_commitment_proof(
            eth_client,
            ibc_contrct_address,
            target_block_number,
            ack_path,
            ibc_contract_slot,
        )
        .await?;
        if storage_proof.value.is_zero() {
            anyhow::bail!("Membership value is empty")
        }

        msg.proof_acked = serde_json::to_vec(&storage_proof)?;
        msg.proof_height = Some(target_height);
        anyhow::Ok(())
    }))
    .await?;

    future::try_join_all(timeout_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().try_into()?;
        let receipt_path = packet.receipt_commitment_path();
        let storage_proof = get_commitment_proof(
            eth_client,
            ibc_contrct_address,
            target_block_number,
            receipt_path,
            ibc_contract_slot,
        )
        .await?;
        if !storage_proof.value.is_zero() {
            anyhow::bail!("Non-Membership value is empty")
        }
        msg.proof_unreceived = serde_json::to_vec(&storage_proof)?;
        msg.proof_height = Some(target_height);
        anyhow::Ok(())
    }))
    .await?;

    Ok(())
}

async fn get_commitment_proof<T: Transport + Clone, P: Provider<T> + Clone>(
    eth_client: &EthApiClient<T, P>,
    ibc_contrct_address: &str,
    block_number: u64,
    path: Vec<u8>,
    slot: U256,
) -> Result<StorageProof> {
    let storage_key = ibc_commitment_key_v2(path, slot);
    let storage_key_hex = format!("0x{storage_key:x}");
    let block_hex = format!("{block_number:x}");

    let proof = eth_client
        .get_proof(ibc_contrct_address, vec![storage_key_hex], block_hex)
        .await?;
    let storage_proof = proof.storage_proof.first().unwrap();

    Ok(StorageProof {
        key: storage_proof.key.as_b256(),
        value: storage_proof.value,
        proof: storage_proof.proof.clone(),
    })
}
