//! IBC event types for the ICS26 router program

use crate::state::{ClientAccount, Packet};
use anchor_lang::prelude::*;

/// Event emitted when a packet is sent
#[event]
#[derive(Debug, Clone)]
pub struct PacketSent {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
    pub timeout_timestamp: i64,
}

/// Event emitted when a packet acknowledgement is written
#[event]
#[derive(Debug, Clone)]
pub struct AcknowledgementWritten {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
    pub acknowledgements: Vec<Vec<u8>>,
}

/// Event emitted when a packet is acknowledged
#[event]
#[derive(Debug, Clone)]
pub struct PacketAcknowledged {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
    pub acknowledgement: Vec<Vec<u8>>,
}

/// Event emitted when a packet times out
#[event]
#[derive(Debug, Clone)]
pub struct PacketTimedOut {
    pub client_id: String,
    pub sequence: u64,
    pub packet: Packet,
}

/// Event emitted when a client is added
#[event]
#[derive(Debug, Clone)]
pub struct ClientAdded {
    pub client: ClientAccount,
}

/// Event emitted when a client is updated
#[event]
#[derive(Debug, Clone)]
pub struct ClientUpdated {
    pub client: ClientAccount,
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
pub struct Noop {}

/// Event emitted when access manager is updated
#[event]
#[derive(Debug, Clone)]
pub struct AccessManagerUpdated {
    pub old_access_manager: Pubkey,
    pub new_access_manager: Pubkey,
}
