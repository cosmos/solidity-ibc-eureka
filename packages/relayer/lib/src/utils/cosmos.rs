//! Relayer utilities for `CosmosSDK` chains.

use alloy::{hex, primitives::U256, providers::Provider};
use anyhow::Result;
use ethereum_apis::{beacon_api::client::BeaconApiClient, eth_api::client::EthApiClient};
use ethereum_light_client::membership::{evm_ics26_commitment_path, MembershipProof};
use ethereum_types::execution::{account_proof::AccountProof, storage_proof::StorageProof};
use futures::future;
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use ibc_proto_eureka::{
    ibc::core::{
        channel::v2::{Acknowledgement, MsgAcknowledgement, MsgRecvPacket, MsgTimeout},
        client::v1::Height,
    },
    Protobuf,
};
use tendermint_rpc::HttpClient;

use crate::events::{EurekaEvent, EurekaEventWithHeight};

/// Converts a list of [`EurekaEvent`]s to a list of [`MsgTimeout`]s.
///
/// # Arguments
/// - `target_events` - The list of target events.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `dst_packet_seqs` - The list of dest packet sequences to filter. If empty, no filtering.
/// - `signer_address` - The signer address.
/// - `now` - The current time.
#[must_use]
pub fn target_events_to_timeout_msgs(
    target_events: Vec<EurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    dst_packet_seqs: &[u64],
    signer_address: &str,
    now: u64,
) -> Vec<MsgTimeout> {
    target_events
        .into_iter()
        .filter_map(|e| match e.event {
            EurekaEvent::SendPacket(packet) => (now >= packet.timeoutTimestamp
                && packet.sourceClient == dst_client_id
                && packet.destClient == src_client_id
                && (dst_packet_seqs.is_empty() || dst_packet_seqs.contains(&packet.sequence)))
            .then_some(MsgTimeout {
                packet: Some(packet.into()),
                proof_height: None,
                proof_unreceived: vec![],
                signer: signer_address.to_string(),
            }),
            EurekaEvent::WriteAcknowledgement(..) => None,
        })
        .collect()
}

/// Converts a list of [`EurekaEvent`]s to a list of [`MsgRecvPacket`]s and
/// [`MsgAcknowledgement`]s.
///
/// # Arguments
/// - `src_events` - The list of source events.
/// - `src_client_id` - The source client ID.
/// - `dst_client_id` - The destination client ID.
/// - `src_packet_seqs` - The list of source packet sequences to filter. If empty, no filtering.
/// - `dst_packet_seqs` - The list of dest packet sequences to filter. If empty, no filtering.
/// - `signer_address` - The signer address.
/// - `now` - The current time.
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn src_events_to_recv_and_ack_msgs(
    src_events: Vec<EurekaEventWithHeight>,
    src_client_id: &str,
    dst_client_id: &str,
    src_packet_seqs: &[u64],
    dst_packet_seqs: &[u64],
    signer_address: &str,
    now: u64,
) -> (Vec<MsgRecvPacket>, Vec<MsgAcknowledgement>) {
    let (src_send_events, src_ack_events): (Vec<_>, Vec<_>) = src_events
        .into_iter()
        .filter(|e| match &e.event {
            EurekaEvent::SendPacket(packet) => {
                packet.timeoutTimestamp > now
                    && packet.sourceClient == src_client_id
                    && packet.destClient == dst_client_id
                    && (src_packet_seqs.is_empty() || src_packet_seqs.contains(&packet.sequence))
            }
            EurekaEvent::WriteAcknowledgement(packet, _) => {
                packet.sourceClient == dst_client_id
                    && packet.destClient == src_client_id
                    && (dst_packet_seqs.is_empty() || dst_packet_seqs.contains(&packet.sequence))
            }
        })
        .partition(|e| match e.event {
            EurekaEvent::SendPacket(_) => true,
            EurekaEvent::WriteAcknowledgement(..) => false,
        });

    let recv_msgs = src_send_events
        .into_iter()
        .map(|e| match e.event {
            EurekaEvent::SendPacket(packet) => MsgRecvPacket {
                packet: Some(packet.into()),
                proof_height: None,
                proof_commitment: vec![],
                signer: signer_address.to_string(),
            },
            EurekaEvent::WriteAcknowledgement(..) => unreachable!(),
        })
        .collect::<Vec<MsgRecvPacket>>();

    let ack_msgs = src_ack_events
        .into_iter()
        .map(|e| match e.event {
            EurekaEvent::WriteAcknowledgement(packet, acks) => MsgAcknowledgement {
                packet: Some(packet.into()),
                acknowledgement: Some(Acknowledgement {
                    app_acknowledgements: acks.into_iter().map(Into::into).collect(),
                }),
                proof_height: None,
                proof_acked: vec![],
                signer: signer_address.to_string(),
            },
            EurekaEvent::SendPacket(_) => unreachable!(),
        })
        .collect::<Vec<MsgAcknowledgement>>();

    (recv_msgs, ack_msgs)
}

/// Generates and injects tendermint proofs for rec, ack and timeout messages.
/// # Errors
/// Returns an error a proof cannot be generated for any of the provided messages.
/// # Panics
/// Panics if the provided messages do not contain a valid packet.
pub async fn inject_tendermint_proofs(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
    source_tm_client: &HttpClient,
    target_height: &Height,
) -> Result<()> {
    future::try_join_all(recv_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let commitment_path = packet.commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(
                &[b"ibc".to_vec(), commitment_path],
                target_height.revision_height,
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
        let packet: Packet = msg.packet.clone().unwrap().into();
        let ack_path = packet.ack_commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(&[b"ibc".to_vec(), ack_path], target_height.revision_height)
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
        let packet: Packet = msg.packet.clone().unwrap().into();
        let receipt_path = packet.receipt_commitment_path();
        let (value, proof) = source_tm_client
            .prove_path(
                &[b"ibc".to_vec(), receipt_path],
                target_height.revision_height,
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

/// Generates and injects Ethereum proofs for rec, ack and timeout messages.
/// # Errors
/// Returns an error if a proof cannot be generated for any of the provided messages.
/// # Panics
/// Panics if the provided messages do not contain a valid packet.
#[allow(clippy::too_many_arguments)]
pub async fn inject_ethereum_proofs<P: Provider + Clone>(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
    eth_client: &EthApiClient<P>,
    beacon_api_client: &BeaconApiClient,
    ibc_contract_address: &str,
    ibc_contract_slot: U256,
    proof_slot: u64,
) -> Result<()> {
    let current_beacon_block = beacon_api_client
        .beacon_block(&format!("{proof_slot:?}"))
        .await?;

    let proof_block_number = current_beacon_block
        .message
        .body
        .execution_payload
        .block_number;

    let proof_slot_height = Height {
        revision_number: 0,
        revision_height: proof_slot,
    };

    let account_proof =
        get_account_proof(eth_client, ibc_contract_address, proof_block_number).await?;

    // recv messages
    future::try_join_all(recv_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let commitment_path = packet.commitment_path();
        let storage_proof = get_storage_proof(
            eth_client,
            ibc_contract_address,
            proof_block_number,
            commitment_path,
            ibc_contract_slot,
        )
        .await?;
        if storage_proof.value.is_zero() {
            anyhow::bail!("Membership value is empty")
        }

        let membership_proof = MembershipProof {
            account_proof: account_proof.clone(),
            storage_proof,
        };
        msg.proof_commitment = serde_json::to_vec(&membership_proof)?;
        msg.proof_height = Some(proof_slot_height);
        anyhow::Ok(())
    }))
    .await?;

    // ack messages
    future::try_join_all(ack_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let ack_path = packet.ack_commitment_path();
        let storage_proof = get_storage_proof(
            eth_client,
            ibc_contract_address,
            proof_block_number,
            ack_path,
            ibc_contract_slot,
        )
        .await?;
        if storage_proof.value.is_zero() {
            anyhow::bail!("Membership value is empty")
        }

        let membership_proof = MembershipProof {
            account_proof: account_proof.clone(),
            storage_proof,
        };
        msg.proof_acked = serde_json::to_vec(&membership_proof)?;
        msg.proof_height = Some(proof_slot_height);
        anyhow::Ok(())
    }))
    .await?;

    // timeout messages
    future::try_join_all(timeout_msgs.iter_mut().map(|msg| async {
        let packet: Packet = msg.packet.clone().unwrap().into();
        let receipt_path = packet.receipt_commitment_path();
        let storage_proof = get_storage_proof(
            eth_client,
            ibc_contract_address,
            proof_block_number,
            receipt_path,
            ibc_contract_slot,
        )
        .await?;
        if !storage_proof.value.is_zero() {
            anyhow::bail!("Non-Membership value is empty")
        }

        let membership_proof = MembershipProof {
            account_proof: account_proof.clone(),
            storage_proof,
        };
        msg.proof_unreceived = serde_json::to_vec(&membership_proof)?;
        msg.proof_height = Some(proof_slot_height);
        anyhow::Ok(())
    }))
    .await?;

    Ok(())
}

async fn get_storage_proof<P: Provider + Clone>(
    eth_client: &EthApiClient<P>,
    ibc_contract_address: &str,
    block_number: u64,
    path: Vec<u8>,
    slot: U256,
) -> Result<StorageProof> {
    let storage_key = evm_ics26_commitment_path(&path, slot);
    let storage_key_be_bytes = storage_key.to_be_bytes_vec();
    let storage_key_hex = hex::encode(storage_key_be_bytes);
    let block_hex = format!("0x{block_number:x}");

    let proof = eth_client
        .get_proof(ibc_contract_address, vec![storage_key_hex], block_hex)
        .await?;
    let storage_proof = proof.storage_proof.first().unwrap();

    Ok(StorageProof {
        key: storage_proof.key.as_b256(),
        value: storage_proof.value,
        proof: storage_proof.proof.clone(),
    })
}

async fn get_account_proof<P: Provider + Clone>(
    eth_client: &EthApiClient<P>,
    ibc_contract_address: &str,
    block_number: u64,
) -> Result<AccountProof> {
    let proof = eth_client
        .get_proof(ibc_contract_address, vec![], format!("0x{block_number:x}"))
        .await?;

    Ok(AccountProof {
        proof: proof.account_proof,
        storage_root: proof.storage_hash,
    })
}

/// Injects mock proofs into the provided messages for testing purposes.
pub fn inject_mock_proofs(
    recv_msgs: &mut [MsgRecvPacket],
    ack_msgs: &mut [MsgAcknowledgement],
    timeout_msgs: &mut [MsgTimeout],
) {
    for msg in recv_msgs.iter_mut() {
        msg.proof_commitment = b"mock".to_vec();
        msg.proof_height = Some(Height::default());
    }

    for msg in ack_msgs.iter_mut() {
        msg.proof_acked = b"mock".to_vec();
        msg.proof_height = Some(Height::default());
    }

    for msg in timeout_msgs.iter_mut() {
        msg.proof_unreceived = b"mock".to_vec();
        msg.proof_height = Some(Height::default());
    }
}
