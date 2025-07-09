//! Solana light client update logic

use crate::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    error::SolanaIBCError,
    header::Header,
};

/// Updates the consensus state with a new header
/// Returns (new_slot, new_consensus_state, optional_new_client_state)
/// # Errors
/// Returns an error if the update cannot be performed
pub fn update_consensus_state(
    current_consensus_state: ConsensusState,
    current_client_state: ClientState,
    header: Header,
) -> Result<(u64, ConsensusState, Option<ClientState>), SolanaIBCError> {
    // Create new consensus state with updated slot and timestamp
    let new_consensus_state = ConsensusState {
        slot: header.new_slot,
        timestamp: header.timestamp,
    };

    // Update client state if the slot has progressed
    let new_client_state = if header.new_slot > current_client_state.latest_slot {
        Some(ClientState {
            latest_slot: header.new_slot,
            ..current_client_state
        })
    } else {
        None
    };

    Ok((header.new_slot, new_consensus_state, new_client_state))
}
