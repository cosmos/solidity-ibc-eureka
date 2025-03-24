//! This module implements the sync protocol helpers defined in [consensus-specs](https://github.com/ethereum/consensus-specs)

use alloy_primitives::B256;
use ethereum_types::consensus::{
    light_client_header::LightClientHeader,
    merkle::{
        floorlog2, CURRENT_SYNC_COMMITTEE_GINDEX, CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA,
        EXECUTION_PAYLOAD_GINDEX, FINALIZED_ROOT_GINDEX, FINALIZED_ROOT_GINDEX_ELECTRA,
        NEXT_SYNC_COMMITTEE_GINDEX, NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA,
    },
};
use tree_hash::TreeHash;

use crate::{client_state::ClientState, error::EthereumIBCError, trie::validate_merkle_branch};

/// Returns the finalized root gindex at the given slot.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md#modified-finalized_root_gindex_at_slot)
/// # Errors
/// Returns an error if the epoch is in a non-supported fork.
pub fn finalized_root_gindex_at_slot(
    client_state: &ClientState,
    slot: u64,
) -> Result<u64, EthereumIBCError> {
    let epoch = client_state.compute_epoch_at_slot(slot);

    client_state.verify_supported_fork_at_epoch(epoch)?;

    if epoch >= client_state.fork_parameters.electra.epoch {
        return Ok(FINALIZED_ROOT_GINDEX_ELECTRA);
    }

    Ok(FINALIZED_ROOT_GINDEX)
}

/// Returns the current sync committee gindex at the given slot.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md#modified-current_sync_committee_gindex_at_slot)
/// # Errors
/// Returns an error if the epoch is in a non-supported fork.
pub fn current_sync_committee_gindex_at_slot(
    client_state: &ClientState,
    slot: u64,
) -> Result<u64, EthereumIBCError> {
    let epoch = client_state.compute_epoch_at_slot(slot);

    client_state.verify_supported_fork_at_epoch(epoch)?;

    if epoch >= client_state.fork_parameters.electra.epoch {
        return Ok(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA);
    }

    Ok(CURRENT_SYNC_COMMITTEE_GINDEX)
}

/// Returns the next sync committee gindex at the given slot.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md#modified-next_sync_committee_gindex_at_slot)
/// # Errors
/// Returns an error if the epoch is in a non-supported fork.
pub fn next_sync_committee_gindex_at_slot(
    client_state: &ClientState,
    slot: u64,
) -> Result<u64, EthereumIBCError> {
    let epoch = client_state.compute_epoch_at_slot(slot);

    client_state.verify_supported_fork_at_epoch(epoch)?;

    if epoch >= client_state.fork_parameters.electra.epoch {
        return Ok(NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA);
    }

    Ok(NEXT_SYNC_COMMITTEE_GINDEX)
}

/// Returns the execution root of the light client header.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md#modified-get_lc_execution_root)
/// # Errors
/// Returns an error if the epoch is less than the Deneb epoch.
pub fn get_lc_execution_root(
    client_state: &ClientState,
    header: &LightClientHeader,
) -> Result<B256, EthereumIBCError> {
    let epoch = client_state.compute_epoch_at_slot(header.beacon.slot);

    client_state.verify_supported_fork_at_epoch(epoch)?;

    // Deneb and electra have the same execution payload header structure, so no need to check or
    // convert the execution payload header.
    Ok(header.execution.tree_hash_root())
}

/// Validates a light client header.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/deneb/light-client/sync-protocol.md#modified-is_valid_light_client_header)
/// # Errors
/// Returns an error if the header cannot be validated.
pub fn is_valid_light_client_header(
    client_state: &ClientState,
    header: &LightClientHeader,
) -> Result<(), EthereumIBCError> {
    let epoch = client_state.compute_epoch_at_slot(header.beacon.slot);

    client_state.verify_supported_fork_at_epoch(epoch)?;

    validate_merkle_branch(
        get_lc_execution_root(client_state, header)?,
        header.execution_branch.to_vec(),
        floorlog2(EXECUTION_PAYLOAD_GINDEX),
        get_subtree_index(EXECUTION_PAYLOAD_GINDEX),
        header.beacon.body_root,
    )
}

/// Values that are constant across all configurations.
/// <https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#get_subtree_index>
#[must_use]
pub const fn get_subtree_index(idx: u64) -> u64 {
    idx % 2_u64.pow(idx.ilog2())
}

/// Normalize a merkle branch to a depth for given gindex.
#[must_use]
pub fn normalize_merkle_branch(branch: Vec<B256>, gindex: u64) -> Vec<B256> {
    // Compute the “depth” from gindex.
    let depth = floorlog2(gindex);
    // If the branch length is shorter than depth, we need to prepend extra default elements.
    if depth > branch.len() {
        let num_extra = depth - branch.len();
        // Create a new vector with num_extra default values followed by the original branch.
        let mut normalized = vec![B256::default(); num_extra];
        normalized.extend(branch);
        normalized
    } else {
        branch
    }
}
