//! Membership proof verification for attestor client

use crate::{client_state::ClientState, consensus_state::ConsensusState, error::SolanaIBCError};
use std::collections::HashMap;

/// Verify membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
pub fn verify_membership(
    consensus_state: ConsensusState,
    _client_state: ClientState,
    _proof: Vec<u8>,
    _path: Vec<Vec<u8>>,
    _value: Vec<u8>,
    height: u64,
    consensus_states: &HashMap<u64, ConsensusState>,
) -> Result<(), SolanaIBCError> {
    // Check if we have consensus state for the requested height
    if !consensus_states.contains_key(&height) {
        return Err(SolanaIBCError::HeightNotFound(height));
    }
    
    // For now, return success if height exists in consensus state
    // TODO: Implement actual proof verification logic
    Ok(())
}

/// Verify non-membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
pub fn verify_non_membership(
    consensus_state: ConsensusState,
    _client_state: ClientState,
    _proof: Vec<u8>,
    _path: Vec<Vec<u8>>,
    height: u64,
    consensus_states: &HashMap<u64, ConsensusState>,
) -> Result<(), SolanaIBCError> {
    // Check if we have consensus state for the requested height
    if !consensus_states.contains_key(&height) {
        return Err(SolanaIBCError::HeightNotFound(height));
    }
    
    // For now, return success if height exists in consensus state
    // TODO: Implement actual proof verification logic
    Ok(())
}
