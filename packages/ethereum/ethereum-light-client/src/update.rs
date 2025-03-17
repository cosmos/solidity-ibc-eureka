//! This module provides [`update_consensus_state`] function to update the consensus state

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::EthereumIBCError,
    header::Header,
};

/// Takes in the current client and consensus state and a new header and returns the updated
/// consensus state and optionally the updated client state (if it needs to be updated)
/// # Errors
/// Returns an error if the store period is not equal to the finalized period
#[allow(clippy::module_name_repetitions, clippy::needless_pass_by_value)]
pub fn update_consensus_state(
    current_consensus_state: ConsensusState,
    current_client_state: ClientState,
    header: Header,
) -> Result<(u64, ConsensusState, Option<ClientState>), EthereumIBCError> {
    let store_period =
        current_client_state.compute_sync_committee_period_at_slot(current_consensus_state.slot);

    let update_finalized_period = current_client_state.compute_sync_committee_period_at_slot(
        header.consensus_update.finalized_header.beacon.slot,
    );

    let mut new_consensus_state = current_consensus_state.clone();
    let mut new_client_state: Option<ClientState> = None;

    if let Some(next_sync_committee) = current_consensus_state.next_sync_committee {
        // sync committee only changes when the period change
        if update_finalized_period == store_period + 1 {
            new_consensus_state.current_sync_committee = next_sync_committee;
            new_consensus_state.next_sync_committee = header
                .consensus_update
                .next_sync_committee
                .map(|c| c.aggregate_pubkey);
        }
    } else {
        // if the finalized period is greater, we have to have a next sync committee
        ensure!(
            update_finalized_period == store_period,
            EthereumIBCError::StorePeriodMustBeEqualToFinalizedPeriod
        );
        new_consensus_state.next_sync_committee = header
            .consensus_update
            .next_sync_committee
            .map(|c| c.aggregate_pubkey);
    }

    let updated_slot = header.consensus_update.finalized_header.beacon.slot;
    if updated_slot > current_consensus_state.slot {
        new_consensus_state.slot = updated_slot;
        new_consensus_state.state_root = header
            .consensus_update
            .finalized_header
            .execution
            .state_root;
        new_consensus_state.storage_root = header.account_update.account_proof.storage_root;
        new_consensus_state.timestamp =
            header.consensus_update.finalized_header.execution.timestamp;

        if updated_slot > current_client_state.latest_slot {
            new_client_state = Some(ClientState {
                latest_slot: updated_slot,
                ..current_client_state
            });
        }
    }

    Ok((updated_slot, new_consensus_state, new_client_state))
}
