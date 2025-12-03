use crate::constants::*;
use anchor_lang::prelude::*;

/// Account schema version
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum AccountVersion {
    V1,
}

/// Main GMP application state
#[account]
#[derive(InitSpace)]
pub struct GMPAppState {
    /// Schema version for upgrades
    pub version: AccountVersion,

    /// Emergency pause flag
    pub paused: bool,

    /// PDA bump seed
    pub bump: u8,

    /// Access manager program ID for role-based access control
    pub access_manager: Pubkey,

    /// Reserved space for future fields
    pub _reserved: [u8; 256],
}

impl GMPAppState {
    pub const SEED: &'static [u8] = solana_ibc_types::GMPAppState::SEED;

    /// Get signer seeds for this app state
    /// Seeds: [`b"app_state`", `GMP_PORT_ID.as_bytes()`, bump]
    pub fn signer_seeds(&self) -> Vec<Vec<u8>> {
        vec![
            Self::SEED.to_vec(),
            GMP_PORT_ID.as_bytes().to_vec(),
            vec![self.bump],
        ]
    }
}

/// Send call message (unvalidated input from user)
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SendCallMsg {
    /// Source client identifier
    pub source_client: String,

    /// Timeout timestamp (unix seconds)
    pub timeout_timestamp: i64,

    /// Receiver address (string format to support any destination chain)
    pub receiver: String,

    /// Account salt
    pub salt: Vec<u8>,

    /// Call payload (instruction data + accounts)
    pub payload: Vec<u8>,

    /// Optional memo
    pub memo: String,
}

// Re-export types from proto crate
pub use crate::proto::{
    GmpAcknowledgement, GmpSolanaPayload, GmpValidationError, SolanaAccountMeta,
};

pub use solana_ibc_types::{CallResultStatus, GMPCallResult};

/// Stores result of a GMP call (ack or timeout) for sender queries
#[account]
#[derive(InitSpace)]
pub struct GMPCallResultAccount {
    pub version: AccountVersion,
    #[max_len(128)]
    pub sender: String,
    pub sequence: u64,
    #[max_len(64)]
    pub source_client: String,
    #[max_len(64)]
    pub dest_client: String,
    pub status: CallResultStatus,
    #[max_len(1024)]
    pub acknowledgement: Vec<u8>,
    pub result_timestamp: i64,
    pub bump: u8,
}
