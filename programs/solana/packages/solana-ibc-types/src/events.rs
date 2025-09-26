//! IBC event types for Solana programs
//!
//! These events are emitted by the ICS26 router and other IBC programs.

use anchor_lang::prelude::*;
use crate::router::Packet;

/// Event emitted when a packet is sent
#[event]
#[derive(Debug, Clone)]
pub struct SendPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
}

/// Event emitted when a packet acknowledgement is written
#[event]
#[derive(Debug, Clone)]
pub struct WriteAcknowledgementEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
    pub acknowledgements: Vec<u8>,
}

/// Event emitted when a packet is acknowledged
#[event]
#[derive(Debug, Clone)]
pub struct AckPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
    pub acknowledgement: Vec<u8>,
}

/// Event emitted when a packet times out
#[event]
#[derive(Debug, Clone)]
pub struct TimeoutPacketEvent {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
}

/// Event emitted when a client is added
#[event]
#[derive(Debug, Clone)]
pub struct ClientAddedEvent {
    pub client_id: String,
    pub client_program_id: Pubkey,
    pub authority: Pubkey,
}

/// Event emitted when a client status is updated
#[event]
#[derive(Debug, Clone)]
pub struct ClientStatusUpdatedEvent {
    pub client_id: String,
    pub active: bool,
}

/// Event emitted when an IBC app is added
#[event]
#[derive(Debug, Clone)]
pub struct IBCAppAdded {
    pub port_id: String,
    pub app_program_id: Pubkey,
}

/// No-op event for testing
#[event]
#[derive(Debug, Clone)]
pub struct NoopEvent {}
