//! Relayer utilities for `CosmosSDK` chains.

use anyhow::Result;
use futures::future;
use ibc_eureka_solidity_types::ics26::router::{SendPacket, WriteAcknowledgement};
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
/// # Errors
/// Returns an error if proof cannot be generated, or membership value is empty for a packet.
pub async fn target_events_to_timeout_msgs(
    target_events: Vec<EurekaEvent>,
    source_tm_client: &HttpClient,
    target_channel_id: &str,
    revision_number: u64,
    target_height: u32,
    signer_address: &str,
) -> Result<Vec<MsgTimeout>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();

    Ok(future::try_join_all(
        target_events
            .into_iter()
            .filter(|e| match e {
                EurekaEvent::SendPacket(se) => {
                    now >= se.packet.timeoutTimestamp
                        && se.packet.sourceChannel == target_channel_id
                }
                _ => false,
            })
            .map(|e| async {
                match e {
                    EurekaEvent::SendPacket(se) => {
                        send_event_to_timout_packet(
                            se,
                            source_tm_client,
                            revision_number,
                            target_height,
                            signer_address.to_string(),
                        )
                        .await
                    }
                    _ => unreachable!(),
                }
            }),
    )
    .await?
    .into_iter()
    .collect::<Vec<_>>())
}

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
async fn send_event_to_timout_packet(
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

/// Converts a [`WriteAcknowledgement`] event to a [`MsgAcknowledgement`].
/// This function doesn't check whether the packet is already acknowledged.
/// # Errors
/// Returns an error if proof cannot be generated, or membership value is empty.
pub async fn write_ack_event_to_ack_packet(
    we: WriteAcknowledgement,
    source_tm_client: &HttpClient,
    revision_number: u64,
    target_height: u32,
    signer_address: String,
) -> Result<MsgAcknowledgement> {
    let ibc_path = we.packet.ack_commitment_path();
    let (value, proof) = source_tm_client
        .prove_path(&[b"ibc".to_vec(), ibc_path], target_height)
        .await?;

    if value.is_empty() {
        anyhow::bail!("Membership value is empty")
    }
    Ok(MsgAcknowledgement {
        packet: Some(we.packet.into()),
        acknowledgement: Some(Acknowledgement {
            app_acknowledgements: we.acknowledgements.into_iter().map(Into::into).collect(),
        }),
        proof_height: Some(Height {
            revision_number,
            revision_height: target_height.into(),
        }),
        proof_acked: proof.encode_vec(),
        signer: signer_address,
    })
}
