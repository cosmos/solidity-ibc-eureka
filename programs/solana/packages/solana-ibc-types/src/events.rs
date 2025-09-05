//! IBC event types for Solana programs
//!
//! These events are emitted by the ICS26 router and other IBC programs.

use anchor_lang::prelude::*;

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
    pub acknowledgements: Vec<Vec<u8>>,
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
