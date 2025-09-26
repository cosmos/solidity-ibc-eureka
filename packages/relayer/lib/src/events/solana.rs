//! Solana IBC event parsing utilities
//!
//! This module provides utilities for parsing IBC events from Solana transaction logs.

use anchor_lang::prelude::*;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use solana_ibc_types::events::{
    AckPacketEvent, SendPacketEvent, TimeoutPacketEvent, WriteAcknowledgementEvent,
};

/// Parsed IBC event from Solana transaction logs
#[derive(Debug, Clone)]
pub enum SolanaEurekaEvent {
    /// A packet was sent
    SendPacket(SendPacketEvent),
    /// An acknowledgement was written for a received packet
    WriteAcknowledgement(WriteAcknowledgementEvent),
    /// A packet acknowledgement was processed
    AckPacket(AckPacketEvent),
    /// A packet timed out
    TimeoutPacket(TimeoutPacketEvent),
}

/// Solana-specific Eureka event with height
#[derive(Debug, Clone)]
pub struct SolanaEurekaEventWithHeight {
    /// The event
    pub event: SolanaEurekaEvent,
    /// The slot height at which the event was emitted
    pub height: u64,
}

/// Parse events from Solana transaction logs
///
/// This function extracts and deserializes Anchor events from the transaction logs.
/// Events are emitted as "Program data: <base64>" in the logs.
/// The data format is: [discriminator (8 bytes)][borsh-serialized event data]
///
/// Returns an error if any IBC event fails to parse, but ignores non-IBC events.
pub fn parse_events_from_logs(logs: &[String]) -> anyhow::Result<Vec<SolanaEurekaEvent>> {
    use anchor_lang::Discriminator;
    use anyhow::{anyhow, Context};

    let mut events = Vec::new();

    for (log_idx, log) in logs.iter().enumerate() {
        if let Some(data_str) = log.strip_prefix("Program data: ") {
            let data = BASE64
                .decode(data_str)
                .with_context(|| format!("Failed to decode base64 in log {}: {}", log_idx, data_str))?;

            if data.len() < 8 {
                // Not an Anchor event, skip
                continue;
            }

            let discriminator = &data[..8];
            let event_data = &data[8..];

            // Check if this is an IBC event we care about
            let is_ibc_event = discriminator == SendPacketEvent::DISCRIMINATOR
                || discriminator == WriteAcknowledgementEvent::DISCRIMINATOR
                || discriminator == AckPacketEvent::DISCRIMINATOR
                || discriminator == TimeoutPacketEvent::DISCRIMINATOR;

            if !is_ibc_event {
                // Not an IBC event, skip without error
                continue;
            }

            // Parse the IBC event - return error if it fails
            let event = match discriminator {
                disc if disc == SendPacketEvent::DISCRIMINATOR => {
                    SendPacketEvent::try_from_slice(event_data)
                        .map(SolanaEurekaEvent::SendPacket)
                        .with_context(|| format!("Failed to deserialize SendPacketEvent in log {}", log_idx))?
                }
                disc if disc == WriteAcknowledgementEvent::DISCRIMINATOR => {
                    WriteAcknowledgementEvent::try_from_slice(event_data)
                        .map(SolanaEurekaEvent::WriteAcknowledgement)
                        .with_context(|| format!("Failed to deserialize WriteAcknowledgementEvent in log {}", log_idx))?
                }
                disc if disc == AckPacketEvent::DISCRIMINATOR => {
                    AckPacketEvent::try_from_slice(event_data)
                        .map(SolanaEurekaEvent::AckPacket)
                        .with_context(|| format!("Failed to deserialize AckPacketEvent in log {}", log_idx))?
                }
                disc if disc == TimeoutPacketEvent::DISCRIMINATOR => {
                    TimeoutPacketEvent::try_from_slice(event_data)
                        .map(SolanaEurekaEvent::TimeoutPacket)
                        .with_context(|| format!("Failed to deserialize TimeoutPacketEvent in log {}", log_idx))?
                }
                _ => {
                    return Err(anyhow!("Unexpected discriminator match in log {}", log_idx));
                }
            };

            events.push(event);
        }
    }

    Ok(events)
}
