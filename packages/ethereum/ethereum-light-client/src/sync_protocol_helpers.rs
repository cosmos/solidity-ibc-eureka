//! This module implements the sync protocol helpers defined in [consensus-specs](https://github.com/ethereum/consensus-specs)

use alloy_primitives::B256;
use ethereum_types::consensus::{
    light_client_header::LightClientHeader,
    merkle::{
        get_subtree_index, CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA, EXECUTION_BRANCH_DEPTH,
        EXECUTION_PAYLOAD_GINDEX, FINALIZED_ROOT_GINDEX_ELECTRA,
        NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA,
    },
    slot::compute_epoch_at_slot,
};
use tree_hash::TreeHash;

use crate::{client_state::ClientState, error::EthereumIBCError, trie::validate_merkle_branch};

/// Returns the finalized root gindex at the given slot.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md#modified-finalized_root_gindex_at_slot)
/// # Errors
/// Returns an error if the epoch is in a non-supported fork.
pub const fn finalized_root_gindex_at_slot(
    client_state: &ClientState,
    slot: u64,
) -> Result<u64, EthereumIBCError> {
    let epoch = compute_epoch_at_slot(client_state.slots_per_epoch, slot);

    // We only support electra fork
    ensure!(
        epoch >= client_state.fork_parameters.electra.epoch,
        EthereumIBCError::MustBeElectra
    );

    Ok(FINALIZED_ROOT_GINDEX_ELECTRA)
}

/// Returns the current sync committee gindex at the given slot.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md#modified-current_sync_committee_gindex_at_slot)
/// # Errors
/// Returns an error if the epoch is in a non-supported fork.
pub const fn current_sync_committee_gindex_at_slot(
    client_state: &ClientState,
    slot: u64,
) -> Result<u64, EthereumIBCError> {
    let epoch = compute_epoch_at_slot(client_state.slots_per_epoch, slot);

    // We only support electra fork
    ensure!(
        epoch >= client_state.fork_parameters.electra.epoch,
        EthereumIBCError::MustBeElectra
    );

    Ok(CURRENT_SYNC_COMMITTEE_GINDEX_ELECTRA)
}

/// Returns the next sync committee gindex at the given slot.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md#modified-next_sync_committee_gindex_at_slot)
/// # Errors
/// Returns an error if the epoch is in a non-supported fork.
pub const fn next_sync_committee_gindex_at_slot(
    client_state: &ClientState,
    slot: u64,
) -> Result<u64, EthereumIBCError> {
    let epoch = compute_epoch_at_slot(client_state.slots_per_epoch, slot);

    // We only support electra fork
    ensure!(
        epoch >= client_state.fork_parameters.electra.epoch,
        EthereumIBCError::MustBeElectra
    );

    Ok(NEXT_SYNC_COMMITTEE_GINDEX_ELECTRA)
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
    let epoch = compute_epoch_at_slot(client_state.slots_per_epoch, header.beacon.slot);

    // We only support electra fork
    ensure!(
        epoch >= client_state.fork_parameters.electra.epoch,
        EthereumIBCError::MustBeElectra
    );

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
    let epoch = compute_epoch_at_slot(client_state.slots_per_epoch, header.beacon.slot);

    // We only support electra fork
    ensure!(
        epoch >= client_state.fork_parameters.electra.epoch,
        EthereumIBCError::MustBeElectra
    );

    // This is required after deneb
    ensure!(
        header.execution.blob_gas_used == 0 && header.execution.excess_blob_gas == 0,
        EthereumIBCError::MissingBlobGas
    );

    validate_merkle_branch(
        get_lc_execution_root(client_state, header)?,
        header.execution_branch.into(),
        EXECUTION_BRANCH_DEPTH,
        get_subtree_index(EXECUTION_PAYLOAD_GINDEX),
        header.beacon.body_root,
    )
}
