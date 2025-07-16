//! Attestor light client verification logic

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::SolanaIBCError,
    header::Header,
};

/// Verifies the header of the light client
///
/// Trusted consensus state must be retvieved using the header
/// height.
///
/// Returns an error if:
/// - The client is frozen
/// - The haeder's timestamp does not match the consensus state
/// - The header's height and trusted height are invalid
/// - The header contains no data
pub fn verify_header(
    trusted_consensus_state: &ConsensusState,
    client_state: &ClientState,
    _current_timestamp: u64,
    header: &Header,
) -> Result<(), SolanaIBCError> {
    // Check if client is frozen
    if client_state.is_frozen {
        return Err(SolanaIBCError::ClientFrozen);
    }

    if header.timestamp != trusted_consensus_state.timestamp {
        return Err(SolanaIBCError::InvalidHeader {
            reason: "timestamp does not match consensus state".into(),
        });
    }

    let height_has_not_progressed = header.new_height <= header.trusted_height;
    if height_has_not_progressed {
        return Err(SolanaIBCError::InvalidHeightProgression {
            current: header.trusted_height,
            new: header.new_height,
        });
    }

    // TODO: Add cryptographic signature verification here
    // For now, we just verify that signature data is present
    if header.signature_data.is_empty() {
        todo!()
    }

    Ok(())
}
