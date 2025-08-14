use anchor_lang::prelude::*;
pub use solana_ibc_app_interface::*;

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
}

/// Constants
pub const APP_STATE_SEED: &[u8] = b"dummy_app_state";

pub const DISCRIMINATOR_SIZE: usize = 8;
pub const PUBKEY_SIZE: usize = 32;
pub const U64_SIZE: usize = 8;

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
