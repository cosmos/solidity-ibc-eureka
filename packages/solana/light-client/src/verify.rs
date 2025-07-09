//! Solana light client verification logic

use crate::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    error::SolanaIBCError,
    header::Header,
};

/// Verifies the header of the light client
/// For now, this is extremely minimal - just basic slot progression
/// # Errors
/// Returns an error if the header cannot be verified
pub fn verify_header(
    consensus_state: &ConsensusState,
    client_state: &ClientState,
    _current_timestamp: u64,
    header: &Header,
) -> Result<(), SolanaIBCError> {
    // Check if client is frozen
    if client_state.is_frozen {
        return Err(SolanaIBCError::ClientFrozen);
    }

    // Verify slot progression (new slot should be greater than trusted slot)
    if header.new_slot <= header.trusted_slot {
        return Err(SolanaIBCError::InvalidSlotProgression {
            current: header.trusted_slot,
            new: header.new_slot,
        });
    }

    // Verify that the trusted slot matches our consensus state
    if header.trusted_slot != consensus_state.slot {
        return Err(SolanaIBCError::InvalidHeader {
            reason: format!(
                "Trusted slot {} does not match consensus state slot {}",
                header.trusted_slot, consensus_state.slot
            ),
        });
    }

    // TODO: Add cryptographic signature verification here
    // For now, we just verify that signature data is present
    if header.signature_data.is_empty() {
        return Err(SolanaIBCError::InvalidSignature);
    }

    Ok(())
}
