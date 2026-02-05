use anchor_lang::prelude::*;
pub use solana_ibc_types::attestation::AccountVersion;

use crate::ETH_ADDRESS_LEN;

/// Attestation light client state.
#[account]
#[derive(InitSpace)]
pub struct ClientState {
    pub version: AccountVersion,
    #[max_len(64)]
    pub client_id: String,
    #[max_len(20)]
    pub attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
    pub min_required_sigs: u8,
    pub latest_height: u64,
    pub is_frozen: bool,
}

impl ClientState {
    pub const SEED: &'static [u8] = b"client";

    pub fn pda(client_id: &str) -> Pubkey {
        Pubkey::find_program_address(&[Self::SEED, client_id.as_bytes()], &crate::ID).0
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace, Eq, PartialEq, Debug)]
pub struct ConsensusState {
    pub height: u64,
    pub timestamp: u64,
}

#[account]
#[derive(InitSpace)]
pub struct AppState {
    pub version: AccountVersion,
    pub access_manager: Pubkey,
    /// Reserved for future upgrades without account migration
    pub _reserved: [u8; 256],
}

impl AppState {
    pub const SEED: &'static [u8] = b"app_state";

    pub fn pda() -> Pubkey {
        Pubkey::find_program_address(&[Self::SEED], &crate::ID).0
    }
}

#[derive(
    AnchorSerialize, AnchorDeserialize, serde::Deserialize, serde::Serialize, Debug, Clone,
)]
pub struct MembershipProof {
    pub attestation_data: Vec<u8>,
    pub signatures: Vec<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct PacketCommitment {
    pub path: [u8; 32],
    pub commitment: [u8; 32],
}

#[derive(Debug, Clone)]
pub struct PacketAttestation {
    pub height: u64,
    pub packets: Vec<PacketCommitment>,
}

#[derive(Debug, Clone)]
pub struct StateAttestation {
    pub height: u64,
    pub timestamp: u64,
}
