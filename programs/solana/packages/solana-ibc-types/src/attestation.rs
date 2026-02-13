//! Attestation light client types for Solana
//!
//! These types define the state for the attestation light client.

use anchor_lang::prelude::*;

/// Account schema version for upgrades
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum AccountVersion {
    V1,
}

/// Client state for the attestation light client
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ClientState {
    pub version: AccountVersion,
    /// Ethereum addresses of trusted attestors (20 bytes each)
    pub attestor_addresses: Vec<[u8; 20]>,
    pub min_required_sigs: u8,
    pub latest_height: u64,
    pub is_frozen: bool,
}

impl ClientState {
    pub const SEED: &'static [u8] = b"client";

    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
    }
}

/// Consensus state for the attestation light client
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ConsensusState {
    pub height: u64,
    /// Timestamp in Unix seconds
    pub timestamp: u64,
}

impl ConsensusState {
    pub const SEED: &'static [u8] = b"consensus_state";

    pub fn pda(height: u64, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, &height.to_le_bytes()], &program_id)
    }
}

/// App state for the attestation light client
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AppState {
    pub version: AccountVersion,
    pub access_manager: Pubkey,
    pub _reserved: [u8; 256],
}

impl AppState {
    pub const SEED: &'static [u8] = b"app_state";

    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
    }
}
