//! Solana-specific types and implementations for combined update-client and membership program

use ibc_client_tendermint_types::{ConsensusState, Header};
use tendermint_light_client_update_client::{SolanaClientState, SolanaUpdateClientOutput};
use tendermint_light_client_membership::{SolanaKVPair, SolanaMembershipOutput};

use crate::UcAndMembershipOutputInfo;

/// Input for the combined update-client and membership program on Solana
#[derive(Clone, Debug)]
pub struct SolanaUcAndMembershipInput {
    /// The client state
    pub client_state: SolanaClientState,
    /// The trusted consensus state
    pub trusted_consensus_state: ConsensusState,
    /// The proposed header
    pub proposed_header: Header,
    /// List of key-value pairs to verify
    pub kv_pairs: Vec<SolanaKVPair>,
    /// Merkle proofs for each key-value pair
    pub merkle_proofs: Vec<Vec<u8>>,
}

/// Output for the combined update-client and membership program on Solana
#[derive(Clone, Debug)]
pub struct SolanaUcAndMembershipOutput {
    /// The update client result
    pub update_client_output: SolanaUpdateClientOutput,
    /// The membership verification result
    pub membership_output: SolanaMembershipOutput,
}

impl UcAndMembershipOutputInfo<SolanaClientState, SolanaKVPair> for SolanaUcAndMembershipOutput {
    type UpdateClientOutput = SolanaUpdateClientOutput;
    type MembershipOutput = SolanaMembershipOutput;

    fn from_results(
        uc_output: Self::UpdateClientOutput,
        membership_output: Self::MembershipOutput,
    ) -> Self {
        Self {
            update_client_output: uc_output,
            membership_output,
        }
    }
}

/// Helper to check if both operations succeeded
pub fn is_verification_successful(output: &SolanaUcAndMembershipOutput) -> bool {
    // Check that the new height was updated
    output.update_client_output.new_height > output.update_client_output.trusted_height
        && !output.membership_output.verified_kv_pairs.is_empty()
}
