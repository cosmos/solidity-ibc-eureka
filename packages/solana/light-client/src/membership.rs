//! Membership proof verification (not implemented for minimal Solana client)

use crate::{client_state::ClientState, consensus_state::ConsensusState, error::SolanaIBCError};

/// Verify membership proof (not implemented)
/// # Errors
/// Always returns unimplemented error
pub fn verify_membership(
    _consensus_state: ConsensusState,
    _client_state: ClientState,
    _proof: Vec<u8>,
    _path: Vec<Vec<u8>>,
    _value: Vec<u8>,
) -> Result<(), SolanaIBCError> {
    Err(SolanaIBCError::InvalidHeader {
        reason: "Membership proofs not implemented for minimal Solana client".to_string(),
    })
}

/// Verify non-membership proof (not implemented)
/// # Errors
/// Always returns unimplemented error
pub fn verify_non_membership(
    _consensus_state: ConsensusState,
    _client_state: ClientState,
    _proof: Vec<u8>,
    _path: Vec<Vec<u8>>,
) -> Result<(), SolanaIBCError> {
    Err(SolanaIBCError::InvalidHeader {
        reason: "Non-membership proofs not implemented for minimal Solana client".to_string(),
    })
}
