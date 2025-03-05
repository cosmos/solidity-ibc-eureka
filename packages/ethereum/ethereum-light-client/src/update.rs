//! This module provides [`update_consensus_state`] function to update the consensus state

use ethereum_types::consensus::{
    slot::compute_timestamp_at_slot, sync_committee::compute_sync_committee_period_at_slot,
};

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::EthereumIBCError,
    header::Header,
};

/// Takes in the current client and consensus state and a new header and returns the
/// updated consensus state and client state.
///
/// The new header must be for a later slot
/// than the current consensus and client state.
/// # Errors
/// Returns an error if the store period is not equal to the finalized period
#[allow(clippy::module_name_repetitions, clippy::needless_pass_by_value)]
pub fn update_consensus_state(
    current_consensus_state: ConsensusState,
    current_client_state: ClientState,
    header: Header,
) -> Result<(u64, ConsensusState, ClientState), EthereumIBCError> {
    let trusted_sync_committee = header.trusted_sync_committee;
    let trusted_slot = trusted_sync_committee.trusted_slot;

    let consensus_update = header.consensus_update;

    let update_slot = consensus_update.attested_header.beacon.slot;

    // We only accept increasing updates:
    ensure!(
        update_slot > trusted_slot,
        EthereumIBCError::TrustedSlotMoreRecentThanUpdateSlot {
            trusted_slot,
            update_slot,
        }
    );
    ensure!(
        update_slot > current_consensus_state.slot,
        EthereumIBCError::CurrentConsensusSlotMoreRecentThanUpdateSlot {
            current_consensus_slot: current_consensus_state.slot,
            update_slot,
        }
    );

    ensure!(
        update_slot > current_client_state.latest_slot,
        EthereumIBCError::CurrentClientStateSlotMoreRecentThanUpdateSlot {
            current_client_state_slot: current_client_state.latest_slot,
            update_slot,
        }
    );

    let store_period = compute_sync_committee_period_at_slot(
        current_client_state.slots_per_epoch,
        current_client_state.epochs_per_sync_committee_period,
        current_consensus_state.slot,
    );

    let update_finalized_period = compute_sync_committee_period_at_slot(
        current_client_state.slots_per_epoch,
        current_client_state.epochs_per_sync_committee_period,
        update_slot,
    );

    let mut new_consensus_state = current_consensus_state.clone();

    // sync committee only changes when the period change
    if update_finalized_period == store_period + 1 {
        new_consensus_state.current_sync_committee = current_consensus_state.next_sync_committee;
        new_consensus_state.next_sync_committee =
            consensus_update.next_sync_committee.aggregate_pubkey;
    }

    new_consensus_state.slot = update_slot;

    new_consensus_state.state_root = consensus_update.attested_header.execution.state_root;
    new_consensus_state.storage_root = header.account_update.account_proof.storage_root;

    new_consensus_state.timestamp = compute_timestamp_at_slot(
        current_client_state.seconds_per_slot,
        current_client_state.genesis_time,
        update_slot,
    );

    let new_client_state = ClientState {
        latest_slot: update_slot,
        ..current_client_state
    };

    Ok((update_slot, new_consensus_state, new_client_state))
}
