//! Attestor light client update logic

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    header::Header,
};

/// Updates the consensus state with a new header
/// Returns (`new_height`, `new_consensus_state`, `optional_new_client_state`)
/// # Errors
/// Returns an error if the update cannot be performed
pub fn update_consensus_state(
    current_client_state: ClientState,
    header: &Header,
) -> Result<(u64, ConsensusState, Option<ClientState>), IbcAttestorClientError> {
    let new_consensus_state = ConsensusState {
        height: header.new_height,
        timestamp: header.timestamp,
    };

    // Update client state if the height has progressed beyond the latest
    let height_has_progressed = header.new_height > current_client_state.latest_height;
    let new_client_state = height_has_progressed.then_some(ClientState {
        latest_height: header.new_height,
        ..current_client_state
    });

    Ok((header.new_height, new_consensus_state, new_client_state))
}
