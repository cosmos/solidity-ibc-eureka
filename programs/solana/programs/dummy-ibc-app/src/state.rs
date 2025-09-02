use anchor_lang::prelude::*;
pub use solana_ibc_types::{
    IBCAppError, OnAcknowledgementPacketMsg, OnRecvPacketMsg, OnTimeoutPacketMsg,
};

/// Dummy IBC App state
#[account]
#[derive(InitSpace)]
pub struct DummyIbcAppState {
    /// Authority that can perform restricted operations
    pub authority: Pubkey,
    /// Counter for received packets (for testing)
    pub packets_received: u64,
    /// Counter for acknowledged packets (for testing)
    pub packets_acknowledged: u64,
    /// Counter for timed out packets (for testing)
    pub packets_timed_out: u64,
    /// Counter for sent packets (for testing transfers)
    pub packets_sent: u64,
}

/// Escrow state to track SOL held for specific transfers
#[account]
#[derive(InitSpace)]
pub struct EscrowState {
    /// Client ID this escrow is for
    #[max_len(64)]
    pub client_id: String,
    /// Total amount currently held in escrow (in lamports)
    pub total_escrowed: u64,
    /// Number of active transfers
    pub active_transfers: u64,
    /// Authority that created this escrow
    pub authority: Pubkey,
}

/// Constants
pub const APP_STATE_SEED: &[u8] = b"dummy_app_state";
pub const ESCROW_STATE_SEED: &[u8] = b"escrow_state";
pub const ESCROW_SEED: &[u8] = b"escrow";
pub const ROUTER_CALLER_SEED: &[u8] = b"router_caller";
pub const TRANSFER_PORT: &str = "transfer";

pub const DISCRIMINATOR_SIZE: usize = 8;
pub const PUBKEY_SIZE: usize = 32;

// Based on actual memory layout observed in tests
pub const PACKETS_RECEIVED_OFFSET: usize = 32;
pub const PACKETS_ACKNOWLEDGED_OFFSET: usize = 40;
pub const PACKETS_TIMED_OUT_OFFSET: usize = 48;

/// Events
#[event]
pub struct PacketReceived {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub acknowledgement: Vec<u8>,
}

#[event]
pub struct PacketAcknowledged {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
    pub acknowledgement: Vec<u8>,
}

#[event]
pub struct PacketTimedOut {
    pub source_client: String,
    pub dest_client: String,
    pub sequence: u64,
}

#[event]
pub struct TransferSent {
    pub sequence: u64,
    pub source_client: String,
    pub denom: String,
    pub amount: String,
    pub sender: String,
    pub receiver: String,
}
