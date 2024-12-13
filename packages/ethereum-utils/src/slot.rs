use crate::error::EthereumUtilsError;

pub const GENESIS_SLOT: u64 = 0;

pub fn compute_slot_at_timestamp(
    genesis_time: u64,
    seconds_per_slot: u64,
    timestamp_seconds: u64,
) -> Result<u64, EthereumUtilsError> {
    checked_compute_slot_at_timestamp(genesis_time, seconds_per_slot, timestamp_seconds).ok_or(
        EthereumUtilsError::FailedToComputeSlotAtTimestamp {
            timestamp: timestamp_seconds,
            genesis: genesis_time,
            seconds_per_slot,
            genesis_slot: GENESIS_SLOT,
        },
    )
}

fn checked_compute_slot_at_timestamp(
    genesis_time: u64,
    seconds_per_slot: u64,
    timestamp_seconds: u64,
) -> Option<u64> {
    timestamp_seconds
        .checked_sub(genesis_time)?
        .checked_div(seconds_per_slot)?
        .checked_add(GENESIS_SLOT)
}

/// Returns the epoch at a given `slot`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#compute_epoch_at_slot)
pub fn compute_epoch_at_slot(slots_per_epoch: u64, slot: u64) -> u64 {
    slot / slots_per_epoch
}

/// Returns the timestamp at a `slot`, respect to `genesis_time`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/bellatrix/beacon-chain.md#compute_timestamp_at_slot)
pub fn compute_timestamp_at_slot(seconds_per_slot: u64, genesis_time: u64, slot: u64) -> u64 {
    let slots_since_genesis = slot - GENESIS_SLOT;
    genesis_time + (slots_since_genesis * seconds_per_slot)
}
