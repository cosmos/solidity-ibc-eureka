//! ICS07 Tendermint light client types for Solana
//!
//! These types define the messages and state for the ICS07 Tendermint light client.

use anchor_lang::prelude::*;

pub use solana_ibc_constants::ASSEMBLE_UPDATE_CLIENT_STATIC_ACCOUNTS;

/// ICS07 Tendermint instruction names and discriminators
pub mod ics07_instructions {
    use crate::utils::compute_discriminator;

    pub const INITIALIZE: &str = "initialize";
    pub const UPLOAD_HEADER_CHUNK: &str = "upload_header_chunk";
    pub const ASSEMBLE_AND_UPDATE_CLIENT: &str = "assemble_and_update_client";
    pub const PRE_VERIFY_SIGNATURE: &str = "pre_verify_signature";
    pub const CLEANUP_INCOMPLETE_UPLOAD: &str = "cleanup_incomplete_upload";

    pub fn initialize_discriminator() -> [u8; 8] {
        compute_discriminator(INITIALIZE)
    }

    pub fn upload_header_chunk_discriminator() -> [u8; 8] {
        compute_discriminator(UPLOAD_HEADER_CHUNK)
    }

    pub fn assemble_and_update_client_discriminator() -> [u8; 8] {
        compute_discriminator(ASSEMBLE_AND_UPDATE_CLIENT)
    }

    pub fn pre_verify_signature_discriminator() -> [u8; 8] {
        compute_discriminator(PRE_VERIFY_SIGNATURE)
    }

    pub fn cleanup_incomplete_upload_discriminator() -> [u8; 8] {
        compute_discriminator(CLEANUP_INCOMPLETE_UPLOAD)
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

/// App state for ICS07 Tendermint
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct AppState {
    pub access_manager: Pubkey,
    pub _reserved: [u8; 256],
}

impl AppState {
    pub const SEED: &'static [u8] = b"app_state";

    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
    }
}

impl ClientState {
    /// ICS07 client state PDA seed
    pub const SEED: &'static [u8] = b"client";

    /// Get ICS07 client state PDA
    pub fn pda(program_id: Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[Self::SEED], &program_id)
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

/// Ed25519 signature data for pre-verification
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct SignatureData {
    pub signature_hash: [u8; 32],
    pub pubkey: [u8; 32],
    pub msg: Vec<u8>,
    pub signature: [u8; 64],
}

/// Offset of `is_valid` field in `SignatureVerification` account data.
/// Equals Anchor discriminator length since `is_valid` is the first field.
pub const SIGNATURE_VERIFICATION_IS_VALID_OFFSET: usize =
    solana_ibc_constants::ANCHOR_DISCRIMINATOR_LEN;
