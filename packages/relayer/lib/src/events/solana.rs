//! Solana IBC event parsing utilities
//!
//! This module provides utilities for parsing IBC events from Solana transaction logs.

use alloy::primitives::Bytes;
use anchor_lang::AnchorDeserialize as _;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::{
    Packet as SolPacket, Payload as SolPayload,
};
use solana_ibc_types::{
    events::{SendPacketEvent, WriteAcknowledgementEvent},
    Packet as SolanaPacket, Payload as SolanaPayload,
};

use crate::events::{EurekaEvent, EurekaEventWithHeight};

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

impl From<&SolanaEurekaEventWithHeight> for EurekaEventWithHeight {
    fn from(event_with_height: &SolanaEurekaEventWithHeight) -> Self {
        let event = match &event_with_height.event {
            SolanaEurekaEvent::SendPacket(send_packet_event) => {
                EurekaEvent::SendPacket(to_sol_packet(&send_packet_event.packet))
            }
            SolanaEurekaEvent::WriteAcknowledgement(write_acknowledgement_event) => {
                EurekaEvent::WriteAcknowledgement(
                    to_sol_packet(&write_acknowledgement_event.packet),
                    write_acknowledgement_event
                        .acknowledgements
                        .clone()
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

fn to_sol_packet(value: &SolanaPacket) -> SolPacket {
    SolPacket {
        sequence: value.sequence,
        sourceClient: value.source_client.clone(),
        destClient: value.dest_client.clone(),
        timeoutTimestamp: u64::try_from(value.timeout_timestamp).unwrap_or(0),
        payloads: value
            .payloads
            .clone()
            .into_iter()
            .map(to_sol_payload)
            .collect(),
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

            tracing::debug!(?event, "parsed event");

            events.push(event);
        }
    }

    Ok(events)
}
