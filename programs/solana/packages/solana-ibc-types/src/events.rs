//! IBC event types for Solana programs
//!
//! These events are emitted by the ICS26 router and can be parsed from
//! Solana transaction logs.

use anchor_lang::prelude::*;

/// Parsed IBC event from Solana transaction logs
#[derive(Debug, Clone)]
pub enum IbcEvent {
    SendPacket(SendPacketEvent),
    WriteAcknowledgement(WriteAcknowledgementEvent),
    AckPacket(AckPacketEvent),
    TimeoutPacket(TimeoutPacketEvent),
    ClientAdded(ClientAddedEvent),
    ClientStatusUpdated(ClientStatusUpdatedEvent),
    IbcAppAdded(IBCAppAdded),
    Noop(NoopEvent),
}

/// Event emitted when a packet is sent
#[event]
#[derive(Debug, Clone)]
pub struct SendPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
}

/// Event emitted when a packet acknowledgement is written
#[event]
#[derive(Debug, Clone)]
pub struct WriteAcknowledgementEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
    pub acknowledgements: Vec<u8>,
}

/// Event emitted when a packet is acknowledged
#[event]
#[derive(Debug, Clone)]
pub struct AckPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
    pub acknowledgement: Vec<u8>,
}

/// Event emitted when a packet times out
#[event]
#[derive(Debug, Clone)]
pub struct TimeoutPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet_data: Vec<u8>,
}

/// Event emitted when a client is added
#[event]
#[derive(Debug, Clone)]
pub struct ClientAddedEvent {
    pub client_id: String,
    pub client_type: String,
}

/// Event emitted when a client status is updated
#[event]
#[derive(Debug, Clone)]
pub struct ClientStatusUpdatedEvent {
    pub client_id: String,
    pub new_status: String,
}

/// Event emitted when an IBC app is added
#[event]
#[derive(Debug, Clone)]
pub struct IBCAppAdded {
    pub port_id: String,
    pub app_address: Pubkey,
}

/// No-op event for testing
#[event]
#[derive(Debug, Clone)]
pub struct NoopEvent {}

/// Parse events from Solana transaction logs
///
/// This function extracts and deserializes Anchor events from the transaction logs.
/// Events are emitted as "Program data: <base64>" in the logs.
/// The data format is: [discriminator (8 bytes)][borsh-serialized event data]
///
/// Invalid events are silently skipped to be resilient to log corruption.
pub fn parse_events_from_logs(logs: &[String]) -> Vec<IbcEvent> {
    use anchor_lang::Discriminator;
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

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
