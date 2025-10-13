//! Solana IBC event parsing utilities
//!
//! This module provides utilities for parsing IBC events from Solana transaction logs.

use alloy::primitives::Bytes;
use anchor_lang::AnchorDeserialize as _;
use anyhow::Context as _;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::{
    Packet as SolPacket, Payload as SolPayload,
};
use solana_ibc_types::{
    events::{SendPacketEvent, WriteAcknowledgementEvent},
    Packet as SolanaPacket, Payload as SolanaPayload,
};

use crate::events::{EurekaEvent, EurekaEventWithHeight};

/// Maximum size for a chunk (matches `CHUNK_DATA_SIZE` in Solana program)
const MAX_CHUNK_SIZE: usize = 700;

/// Parsed IBC event from Solana transaction logs
#[derive(Debug, Clone)]
pub enum SolanaEurekaEvent {
    /// A packet was sent
    SendPacket(SendPacketEvent),
    /// An acknowledgement was written for a received packet
    WriteAcknowledgement(WriteAcknowledgementEvent),
}

/// Solana-specific Eureka event with height
#[derive(Debug, Clone)]
pub struct SolanaEurekaEventWithHeight {
    /// The event
    pub event: SolanaEurekaEvent,
    /// The slot height at which the event was emitted
    pub height: u64,
}

impl From<SolanaEurekaEventWithHeight> for EurekaEventWithHeight {
    fn from(event_with_height: SolanaEurekaEventWithHeight) -> Self {
        let event = match event_with_height.event {
            SolanaEurekaEvent::SendPacket(send_packet_event) => {
                EurekaEvent::SendPacket(to_sol_packet(send_packet_event.packet))
            }
            SolanaEurekaEvent::WriteAcknowledgement(write_acknowledgement_event) => {
                EurekaEvent::WriteAcknowledgement(
                    to_sol_packet(write_acknowledgement_event.packet),
                    write_acknowledgement_event
                        .acknowledgements
                        .into_iter()
                        .map(Bytes::from)
                        .collect(),
                )
            }
        };

        Self {
            event,
            height: event_with_height.height,
        }
    }
}

/// Converts a Solana timeout packet message to a Tendermint timeout message.
///
/// # Arguments
/// * `msg` - The Solana timeout packet message
/// * `signer` - The signer address string
///
/// # Returns
/// A Tendermint `MsgTimeout` for processing by IBC
///
/// # Errors
/// * this function will return error if timeout timestamp is 0 or negative
pub fn solana_timeout_packet_to_tm_timeout(
    msg: solana_ibc_types::MsgTimeoutPacket,
    signer: String,
) -> anyhow::Result<ibc_proto_eureka::ibc::core::channel::v2::MsgTimeout> {
    let packet = ibc_proto_eureka::ibc::core::channel::v2::Packet {
        sequence: msg.packet.sequence,
        source_client: msg.packet.source_client.clone(),
        destination_client: msg.packet.dest_client.clone(),
        timeout_timestamp: u64::try_from(msg.packet.timeout_timestamp)
            .context("timeout should be u64 compatible")?,
        payloads: msg
            .packet
            .payloads
            .into_iter()
            .map(|p| ibc_proto_eureka::ibc::core::channel::v2::Payload {
                source_port: p.source_port,
                destination_port: p.dest_port,
                version: p.version,
                encoding: p.encoding,
                value: p.value,
            })
            .collect(),
    };

    let height = ibc_proto_eureka::ibc::core::client::v1::Height {
        revision_number: 0,                // Solana doesn't have revision numbers
        revision_height: msg.proof.height, // Use ProofMetadata height
    };

    // TODO: Extract actual proof data from chunks if needed
    let proof_unreceived = vec![]; // Placeholder - actual proof would be assembled from chunks

    let msg = ibc_proto_eureka::ibc::core::channel::v2::MsgTimeout {
        proof_unreceived,
        proof_height: Some(height),
        packet: Some(packet),
        signer,
    };

    Ok(msg)
}

/// Converts a Tendermint timeout message to a Solana timeout packet message.
///
/// # Arguments
/// * `msg` - The Tendermint timeout message
///
/// # Returns
/// A Solana `MsgTimeoutPacket` for processing by Solana IBC
///
/// # Errors
/// * Returns error if the packet field is missing
/// * Returns error if the `proof_height` field is missing
/// * Returns error if `timeout_timestamp` cannot be converted to i64
pub fn tm_timeout_to_solana_timeout_packet(
    msg: ibc_proto_eureka::ibc::core::channel::v2::MsgTimeout,
) -> anyhow::Result<solana_ibc_types::MsgTimeoutPacket> {
    let packet = msg.packet.context("packet field is required")?;
    let proof_height = msg.proof_height.context("proof_height field is required")?;

    let solana_packet = solana_ibc_types::Packet {
        sequence: packet.sequence,
        source_client: packet.source_client.clone(),
        dest_client: packet.destination_client.clone(),
        timeout_timestamp: i64::try_from(packet.timeout_timestamp)
            .context("timeout_timestamp should be i64 compatible")?,
        payloads: packet
            .payloads
            .iter()
            .map(|p| solana_ibc_types::Payload {
                source_port: p.source_port.clone(),
                dest_port: p.destination_port.clone(),
                version: p.version.clone(),
                encoding: p.encoding.clone(),
                value: p.value.clone(),
            })
            .collect(),
    };

    // Convert payloads to metadata
    let payload_metadata: Vec<solana_ibc_types::PayloadMetadata> = packet
        .payloads
        .into_iter()
        .map(|p| {
            // Calculate commitment and total chunks for each payload
            let commitment = solana_sdk::keccak::hash(&p.value).0;
            let total_chunks = if p.value.len() > MAX_CHUNK_SIZE {
                u8::try_from(p.value.len().div_ceil(MAX_CHUNK_SIZE)).context("payload too big")?
            } else {
                0
            };

            anyhow::Ok(solana_ibc_types::PayloadMetadata {
                source_port: p.source_port,
                dest_port: p.destination_port,
                version: p.version,
                encoding: p.encoding,
                commitment,
                total_chunks,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    // Create proof metadata
    let proof_commitment = solana_sdk::keccak::hash(&msg.proof_unreceived).0;
    let proof_total_chunks = if msg.proof_unreceived.len() > MAX_CHUNK_SIZE {
        u8::try_from(msg.proof_unreceived.len().div_ceil(MAX_CHUNK_SIZE))
            .context("proof too big")?
    } else {
        0
    };

    let proof_metadata = solana_ibc_types::ProofMetadata {
        height: proof_height.revision_height,
        commitment: proof_commitment,
        total_chunks: proof_total_chunks,
    };

    let msg = solana_ibc_types::MsgTimeoutPacket {
        packet: solana_packet,
        payloads: payload_metadata,
        proof: proof_metadata,
    };

    Ok(msg)
}

fn to_sol_packet(value: SolanaPacket) -> SolPacket {
    SolPacket {
        sequence: value.sequence,
        sourceClient: value.source_client.clone(),
        destClient: value.dest_client.clone(),
        timeoutTimestamp: u64::try_from(value.timeout_timestamp).unwrap_or(0),
        payloads: value.payloads.into_iter().map(to_sol_payload).collect(),
    }
}

fn to_sol_payload(value: SolanaPayload) -> SolPayload {
    SolPayload {
        sourcePort: value.source_port,
        destPort: value.dest_port,
        version: value.version,
        encoding: value.encoding,
        value: value.value.into(),
    }
}

/// Parse events from Solana transaction logs
///
/// This function extracts and deserializes Anchor events from the transaction logs.
///
/// Events are emitted as "Program data: <base64>" in the logs.
/// The data format is: [discriminator (8 bytes)][borsh-serialized event data]
///
/// Returns an error if any IBC event fails to parse, but ignores non-IBC events.
///
/// # Errors
///
/// This function will return an error if:
/// - Base64 decoding fails for any "Program data:" log entry
/// - Deserialization fails for any recognized IBC event (`SendPacket`, `WriteAcknowledgement`)
/// - An impossible discriminator match occurs (internal logic error)
///
/// Non-IBC events and logs without "Program data:" prefix are silently skipped.
/// TODO: Might be easier to parse via `anchor_client` but dependencies get kinda messy so manual parse
pub fn parse_events_from_logs(logs: &[String]) -> anyhow::Result<Vec<SolanaEurekaEvent>> {
    use anchor_lang::Discriminator;
    use anyhow::{anyhow, Context};

    let mut events = Vec::new();

    for (log_idx, log) in logs.iter().enumerate() {
        if let Some(data_str) = log.strip_prefix("Program data: ") {
            let data = BASE64
                .decode(data_str)
                .with_context(|| format!("Failed to decode base64 in log {log_idx}: {data_str}"))?;

            if data.len() < 8 {
                // Not an Anchor event, skip
                continue;
            }

            let discriminator = &data[..8];
            let event_data = &data[8..];

            // Check if this is an IBC event we care about
            let is_ibc_event = discriminator == SendPacketEvent::DISCRIMINATOR
                || discriminator == WriteAcknowledgementEvent::DISCRIMINATOR;

            if !is_ibc_event {
                continue;
            }

            let event = match discriminator {
                disc if disc == SendPacketEvent::DISCRIMINATOR => {
                    SendPacketEvent::try_from_slice(event_data)
                        .map(SolanaEurekaEvent::SendPacket)
                        .with_context(|| {
                            format!("Failed to deserialize SendPacketEvent in log {log_idx}")
                        })?
                }
                disc if disc == WriteAcknowledgementEvent::DISCRIMINATOR => {
                    WriteAcknowledgementEvent::try_from_slice(event_data)
                        .map(SolanaEurekaEvent::WriteAcknowledgement)
                        .with_context(|| {
                            format!(
                                "Failed to deserialize WriteAcknowledgementEvent in log {log_idx}",
                            )
                        })?
                }
                _ => {
                    return Err(anyhow!("Unexpected discriminator match in log {log_idx}"));
                }
            };

            tracing::info!(?event, "parsed event");

            // Debug: Log payload details for parsed events
            match &event {
                SolanaEurekaEvent::SendPacket(send_event) => {
                    tracing::debug!(
                        "SendPacketEvent: sequence={}, {} payloads",
                        send_event.sequence,
                        send_event.packet.payloads.len()
                    );
                    for (i, payload) in send_event.packet.payloads.iter().enumerate() {
                        tracing::debug!(
                            "  Payload {}: source_port={}, dest_port={}, value_len={}",
                            i,
                            payload.source_port,
                            payload.dest_port,
                            payload.value.len()
                        );
                    }
                }
                SolanaEurekaEvent::WriteAcknowledgement(ack_event) => {
                    tracing::debug!(
                        "WriteAcknowledgementEvent: sequence={}, {} payloads, {} acks",
                        ack_event.sequence,
                        ack_event.packet.payloads.len(),
                        ack_event.acknowledgements.len()
                    );
                    for (i, payload) in ack_event.packet.payloads.iter().enumerate() {
                        tracing::debug!(
                            "  Payload {}: source_port={}, dest_port={}, value_len={}",
                            i,
                            payload.source_port,
                            payload.dest_port,
                            payload.value.len()
                        );
                    }
                }
            }

            events.push(event);
        }
    }

    Ok(events)
}
