//! ICS07 Tendermint light client types for Solana
//!
//! These types define the messages and state for the ICS07 Tendermint light client.

use anchor_lang::prelude::*;

/// ICS07 Tendermint instruction names and discriminators
pub mod ics07_instructions {
    use crate::utils::compute_discriminator;

    pub const INITIALIZE: &str = "initialize";
    pub const UPLOAD_HEADER_CHUNK: &str = "upload_header_chunk";
    pub const ASSEMBLE_AND_UPDATE_CLIENT: &str = "assemble_and_update_client";

    pub fn initialize_discriminator() -> [u8; 8] {
        compute_discriminator(INITIALIZE)
    }

    pub fn upload_header_chunk_discriminator() -> [u8; 8] {
        compute_discriminator(UPLOAD_HEADER_CHUNK)
    }

    pub fn assemble_and_update_client_discriminator() -> [u8; 8] {
        compute_discriminator(ASSEMBLE_AND_UPDATE_CLIENT)
    }
}

/// Update client message for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct UpdateClientMsg {
    pub client_message: Vec<u8>, // Serialized Tendermint header
}

/// IBC height structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct IbcHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}

/// Client state for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ClientState {
    pub chain_id: String,
    pub trust_level_numerator: u64,
    pub trust_level_denominator: u64,
    pub trusting_period: u64,
    pub unbonding_period: u64,
    pub max_clock_drift: u64,
    pub frozen_height: IbcHeight,
    pub latest_height: IbcHeight,
}

impl ClientState {
    /// ICS07 client state PDA seed
    pub const SEED: &'static [u8] = b"client";

    /// Get ICS07 client state PDA
    pub fn pda(chain_id: &str, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED, chain_id.as_bytes()], &program_id)
    }
}

/// Consensus state for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ConsensusState {
    /// Timestamp in nanoseconds since Unix epoch
    pub timestamp: u64,
    pub root: [u8; 32],
    pub next_validators_hash: [u8; 32],
}

impl ConsensusState {
    /// ICS07 consensus state PDA seed
    pub const SEED: &'static [u8] = b"consensus_state";

    /// Get ICS07 consensus state PDA
    pub fn pda(client_state: Pubkey, height: u64, program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[Self::SEED, client_state.as_ref(), &height.to_le_bytes()],
            &program_id,
        )
    }
}
