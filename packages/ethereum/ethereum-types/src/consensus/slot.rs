//! This module provides functions to compute slot and epoch from timestamp.

/// Returns the computed slot at a given `timestamp_seconds`.
#[must_use]
pub fn compute_slot_at_timestamp(
    genesis_time: u64,
    genesis_slot: u64,
    seconds_per_slot: u64,
    timestamp_seconds: u64,
) -> Option<u64> {
    timestamp_seconds
        .checked_sub(genesis_time)?
        .checked_div(seconds_per_slot)?
        .checked_add(genesis_slot)
}

/// Returns the epoch at a given `slot`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#compute_epoch_at_slot)
#[allow(clippy::module_name_repetitions)]
#[must_use]
pub const fn compute_epoch_at_slot(slots_per_epoch: u64, slot: u64) -> u64 {
    slot / slots_per_epoch
}

/// Returns the timestamp at a `slot`, respect to `genesis_time`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/bellatrix/beacon-chain.md#compute_timestamp_at_slot)
#[allow(clippy::module_name_repetitions)]
#[must_use]
pub const fn compute_timestamp_at_slot(
    genesis_time: u64,
    genesis_slot: u64,
    seconds_per_slot: u64,
    slot: u64,
) -> u64 {
    let slots_since_genesis = slot - genesis_slot;
    genesis_time + (slots_since_genesis * seconds_per_slot)
}
