//! This module provides [`update_consensus_state`] function to update the consensus state

use ethereum_utils::{ensure, slot::compute_timestamp_at_slot};

use crate::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    error::EthereumIBCError,
    types::{
        height::Height, light_client::Header, sync_committee::compute_sync_committee_period_at_slot,
    },
};

/// Takes in the current client and consensus state and a new header and returns the updated
/// consensus state and optionally the updated client state (if it needs to be updated)
/// # Errors
/// Returns an error if the store period is not equal to the finalized period
#[allow(clippy::module_name_repetitions)]
#[must_use = "the current client and consensus state are not updated in place, but new ones are returned"]
pub fn update_consensus_state(
    current_consensus_state: &ConsensusState,
    current_client_state: &ClientState,
    header: Header,
) -> Result<(Height, ConsensusState, Option<ClientState>), EthereumIBCError> {
    let trusted_sync_committee = header.trusted_sync_committee;
    let trusted_height = trusted_sync_committee.trusted_height;

    let mut new_consensus_state = current_consensus_state.clone();
    let mut new_client_state: Option<ClientState> = None;

    let consensus_update = header.consensus_update;
    let account_update = header.account_update;

    let store_period = compute_sync_committee_period_at_slot(
        current_client_state.slots_per_epoch,
        current_client_state.epochs_per_sync_committee_period,
        new_consensus_state.slot,
    );

    let update_finalized_period = compute_sync_committee_period_at_slot(
        current_client_state.slots_per_epoch,
        current_client_state.epochs_per_sync_committee_period,
        consensus_update.attested_header.beacon.slot,
    );

    if let Some(ref next_sync_committee) = new_consensus_state.next_sync_committee {
        // sync committee only changes when the period change
        if update_finalized_period == store_period + 1 {
            new_consensus_state.current_sync_committee = *next_sync_committee;
            new_consensus_state.next_sync_committee = consensus_update
                .next_sync_committee
                .map(|c| c.aggregate_pubkey);
        }
    } else {
        // if the finalized period is greater, we have to have a next sync committee
        ensure!(
            update_finalized_period == store_period,
            EthereumIBCError::StorePeriodMustBeEqualToFinalizedPeriod
        );
        new_consensus_state.next_sync_committee = consensus_update
            .next_sync_committee
            .map(|c| c.aggregate_pubkey);
    }

    // Some updates can be only for updating the sync committee, therefore the slot number can be
    // smaller. We don't want to save a new state if this is the case.
    let updated_height = core::cmp::max(
        trusted_height.revision_height,
        consensus_update.attested_header.beacon.slot,
    );

    if consensus_update.attested_header.beacon.slot > new_consensus_state.slot {
        new_consensus_state.slot = consensus_update.attested_header.beacon.slot;

        new_consensus_state.state_root = consensus_update.attested_header.execution.state_root;
        new_consensus_state.storage_root = account_update.account_proof.storage_root;

        new_consensus_state.timestamp = compute_timestamp_at_slot(
            current_client_state.seconds_per_slot,
            current_client_state.genesis_time,
            consensus_update.attested_header.beacon.slot,
        );

        if current_client_state.latest_slot < consensus_update.attested_header.beacon.slot {
            let mut updated_client_state = current_client_state.clone();
            updated_client_state.latest_slot = consensus_update.attested_header.beacon.slot;
            new_client_state = Some(updated_client_state);
        }
    }

    let updated_height = Height {
        revision_number: trusted_height.revision_number,
        revision_height: updated_height,
    };

    //save_consensus_state::<Self>(deps, consensus_state, &updated_height);

    //Ok(vec![updated_height])
    Ok((updated_height, new_consensus_state, new_client_state))
}
