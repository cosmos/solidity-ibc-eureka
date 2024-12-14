//! Relayer utilities for `CosmosSDK` chains.

use anyhow::Result;
use ibc_eureka_solidity_types::ics26::router::SendPacket;
use ibc_proto_eureka::{
    ibc::core::{
        channel::v2::{MsgRecvPacket, MsgTimeout},
        client::v1::Height,
    },
    Protobuf,
};
use sp1_ics07_tendermint_utils::rpc::TendermintRpcExt;
use tendermint_rpc::HttpClient;

/// Converts a [`SendPacket`] event to a [`MsgRecvPacket`].
/// This function doesn't check whether the packet is already received or timed out.
/// # Errors
/// Returns an error if proof cannot be generated, or membership value is empty.
pub async fn send_event_to_recv_packet(
    se: SendPacket,
    source_tm_client: &HttpClient,
    revision_number: u64,
    target_height: u32,
    signer_address: String,
) -> Result<MsgRecvPacket> {
    let ibc_path = se.packet.commitment_path();
    let (value, proof) = source_tm_client
        .prove_path(&[b"ibc".to_vec(), ibc_path], target_height)
        .await?;

    if value.is_empty() {
        anyhow::bail!("Membership value is empty")
    }
    Ok(MsgRecvPacket {
        packet: Some(se.packet.into()),
        proof_height: Some(Height {
            revision_number,
            revision_height: target_height.into(),
        }),
        proof_commitment: proof.encode_vec(),
        signer: signer_address,
    })
}

/// Converts a [`SendPacket`] event to a [`MsgTimeout`].
/// This function doesn't check whether the packet is already received or timed out.
/// # Errors
/// Returns an error if proof cannot be generated, or non-membership value is not empty.
pub async fn send_event_to_timout_packet(
    se: SendPacket,
    source_tm_client: &HttpClient,
    revision_number: u64,
    target_height: u32,
    signer_address: String,
) -> Result<MsgTimeout> {
    let ibc_path = se.packet.receipt_commitment_path();
    let (value, proof) = source_tm_client
        .prove_path(&[b"ibc".to_vec(), ibc_path], target_height)
        .await?;

    if !value.is_empty() {
        anyhow::bail!("Non-membership value is not empty")
    }
    Ok(MsgTimeout {
        packet: Some(se.packet.into()),
        proof_height: Some(Height {
            revision_number,
            revision_height: target_height.into(),
        }),
        proof_unreceived: proof.encode_vec(),
        signer: signer_address,
    })
}
