use anchor_lang::prelude::*;
pub use solana_ibc_types::attestation::AccountVersion;

use crate::ETH_ADDRESS_LEN;

/// Client state for the attestation light client
#[account]
#[derive(InitSpace)]
pub struct ClientState {
    pub version: AccountVersion,
    #[max_len(64)]
    pub client_id: String,
    /// Ethereum addresses of trusted attestors (20 bytes each)
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

/// Consensus state for the attestation light client
#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace, Eq, PartialEq, Debug)]
pub struct ConsensusState {
    pub height: u64,
    /// Timestamp in Unix seconds
    pub timestamp: u64,
}

/// App state for access control
#[account]
#[derive(InitSpace)]
pub struct AppState {
    pub version: AccountVersion,
    pub access_manager: Pubkey,
    pub _reserved: [u8; 256],
}

impl AppState {
    pub const SEED: &'static [u8] = b"app_state";

    pub fn pda() -> Pubkey {
        Pubkey::find_program_address(&[Self::SEED], &crate::ID).0
    }
}

/// Membership proof structure containing attestation data and signatures.
/// Uses borsh for efficient binary serialization (vs JSON which is ~2.5x larger).
#[derive(
    AnchorSerialize, AnchorDeserialize, serde::Deserialize, serde::Serialize, Debug, Clone,
)]
pub struct MembershipProof {
    /// ABI-encoded `PacketAttestation` data
    pub attestation_data: Vec<u8>,
    /// 65-byte signatures (r||s||v format)
    pub signatures: Vec<Vec<u8>>,
}

/// ABI-decoded packet commitment structure
#[derive(Debug, Clone)]
pub struct PacketCommitment {
    /// keccak256 hash of the path
    pub path: [u8; 32],
    /// The commitment value
    pub commitment: [u8; 32],
}

/// ABI-decoded `PacketAttestation` structure.
/// Matches Solidity: `PacketAttestation { uint64 height; PacketCompact[] packets; }`
#[derive(Debug, Clone)]
pub struct PacketAttestation {
    pub height: u64,
    pub packets: Vec<PacketCommitment>,
}

/// ABI-decoded `StateAttestation` structure (used for `update_client`).
/// Matches Solidity: `StateAttestation { uint64 height; uint64 timestamp; }`
#[derive(Debug, Clone)]
pub struct StateAttestation {
    pub height: u64,
    pub timestamp: u64,
}
