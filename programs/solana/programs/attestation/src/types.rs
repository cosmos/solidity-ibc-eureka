use anchor_lang::prelude::*;

/// Account schema version for upgrades
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum AccountVersion {
    V1,
}

mod sol_types {
    alloy_sol_types::sol!(
        "../../../../contracts/light-clients/attestation/msgs/IAttestationMsgs.sol"
    );
}

pub use sol_types::IAttestationMsgs::{PacketAttestation, PacketCompact, StateAttestation};

use crate::ETH_ADDRESS_LEN;

/// Attestation light client state.
#[account]
#[derive(InitSpace)]
pub struct ClientState {
    pub version: AccountVersion,
    /// 20-byte Ethereum addresses of trusted attestors.
    #[max_len(20)]
    pub attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
    /// Minimum number of valid attestor signatures required for verification.
    pub min_required_sigs: u8,
    /// Highest block height for which a consensus state has been stored.
    pub latest_height: u64,
    /// Whether the client has been frozen due to misbehaviour detection.
    pub is_frozen: bool,
}

impl ClientState {
    pub const SEED: &'static [u8] = b"client";

    pub fn pda() -> Pubkey {
        Pubkey::find_program_address(&[Self::SEED], &crate::ID).0
    }
}

/// Global program configuration.
#[account]
#[derive(InitSpace)]
pub struct AppState {
    pub version: AccountVersion,
    /// Program ID of the access manager that controls admin operations.
    pub access_manager: Pubkey,
    /// Reserved for future upgrades without account migration.
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
    /// ABI-encoded attestation payload (`PacketAttestation` or `StateAttestation`).
    pub attestation_data: Vec<u8>,
    /// 65-byte ECDSA signatures (r || s || v) over `sha256(attestation_data)`.
    pub signatures: Vec<Vec<u8>>,
}
