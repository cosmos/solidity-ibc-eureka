//! Solana IBC event parsing utilities
//!
//! This module provides utilities for parsing IBC events from Solana transaction logs.

use anchor_lang::prelude::*;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use solana_ibc_types::{
    AckPacketEvent, ClientAddedEvent, ClientStatusUpdatedEvent, IBCAppAdded, NoopEvent,
    SendPacketEvent, TimeoutPacketEvent, WriteAcknowledgementEvent,
};

/// Parsed IBC event from Solana transaction logs
#[derive(Debug, Clone)]
pub enum IbcEvent {
    /// A packet was sent
    SendPacket(SendPacketEvent),
    /// An acknowledgement was written for a received packet
    WriteAcknowledgement(WriteAcknowledgementEvent),
    /// A packet acknowledgement was processed
    AckPacket(AckPacketEvent),
    /// A packet timed out
    TimeoutPacket(TimeoutPacketEvent),
}

/// Parse events from Solana transaction logs
///
/// This function extracts and deserializes Anchor events from the transaction logs.
/// Events are emitted as "Program data: <base64>" in the logs.
/// The data format is: [discriminator (8 bytes)][borsh-serialized event data]
///
/// Invalid events are silently skipped to be resilient to log corruption.
pub fn parse_events_from_logs(logs: &[String]) -> Vec<IbcEvent> {
    use anchor_lang::Discriminator;

    let mut events = Vec::new();

    for log in logs {
        if let Some(data_str) = log.strip_prefix("Program data: ") {
            // Skip invalid base64 data
            if let Ok(data) = BASE64.decode(data_str) {
                if data.len() >= 8 {
                    // Extract discriminator that Anchor already included
                    let discriminator = &data[..8];
                    let event_data = &data[8..];

                    // Match against the discriminators and deserialize the event
                    let event = match discriminator {
                        disc if disc == SendPacketEvent::DISCRIMINATOR => {
                            SendPacketEvent::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::SendPacket)
                        }
                        disc if disc == WriteAcknowledgementEvent::DISCRIMINATOR => {
                            WriteAcknowledgementEvent::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::WriteAcknowledgement)
                        }
                        disc if disc == AckPacketEvent::DISCRIMINATOR => {
                            AckPacketEvent::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::AckPacket)
                        }
                        disc if disc == TimeoutPacketEvent::DISCRIMINATOR => {
                            TimeoutPacketEvent::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::TimeoutPacket)
                        }
                        disc if disc == ClientAddedEvent::DISCRIMINATOR => {
                            ClientAddedEvent::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::ClientAdded)
                        }
                        disc if disc == ClientStatusUpdatedEvent::DISCRIMINATOR => {
                            ClientStatusUpdatedEvent::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::ClientStatusUpdated)
                        }
                        disc if disc == IBCAppAdded::DISCRIMINATOR => {
                            IBCAppAdded::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::IbcAppAdded)
                        }
                        disc if disc == NoopEvent::DISCRIMINATOR => {
                            NoopEvent::try_from_slice(event_data)
                                .ok()
                                .map(IbcEvent::Noop)
                        }
                        _ => None, // Unknown events are skipped
                    };

                    if let Some(e) = event {
                        events.push(e);
                    }
                }
            }
        }
    }

    events
}
