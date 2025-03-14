//! This module defines [`ClientState`].

use alloy_primitives::{Address, B256, U256};
use ethereum_types::consensus::fork::ForkParameters;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The ethereum client state
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct ClientState {
    /// The chain ID
    pub chain_id: u64,
    /// The genesis validators root
    #[schemars(with = "String")]
    pub genesis_validators_root: B256,
    /// The minimum number of participants in the sync committee
    pub min_sync_committee_participants: u64,
    /// The time of genesis (unix timestamp)
    pub genesis_time: u64,
    /// The genesis slot
    pub genesis_slot: u64,
    /// The fork parameters
    pub fork_parameters: ForkParameters,
    /// The slot duration in seconds
    pub seconds_per_slot: u64,
    /// The number of slots per epoch
    pub slots_per_epoch: u64,
    /// The number of epochs per sync committee period
    pub epochs_per_sync_committee_period: u64,
    /// The latest slot of this client
    pub latest_slot: u64,
    /// Whether the client is frozen
    pub is_frozen: bool,
    /// The address of the IBC contract being tracked on Ethereum
    #[schemars(with = "String")]
    pub ibc_contract_address: Address,
    /// The storage slot of the IBC commitment in the Ethereum contract
    #[schemars(with = "String")]
    pub ibc_commitment_slot: U256,
}

impl ClientState {
    /// Returns the computed slot at a given `timestamp_seconds`.
    #[must_use]
    pub fn compute_slot_at_timestamp(&self, timestamp_seconds: u64) -> Option<u64> {
        timestamp_seconds
            .checked_sub(self.genesis_time)?
            .checked_div(self.seconds_per_slot)?
            .checked_add(self.genesis_slot)
    }

    /// Returns the epoch at a given `slot`.
    ///
    /// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/phase0/beacon-chain.md#compute_epoch_at_slot)
    #[allow(clippy::module_name_repetitions)]
    #[must_use]
    pub const fn compute_epoch_at_slot(&self, slot: u64) -> u64 {
        slot / self.slots_per_epoch
    }

    /// Returns the timestamp at a `slot`, respect to `genesis_time`.
    ///
    /// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/bellatrix/beacon-chain.md#compute_timestamp_at_slot)
    #[allow(clippy::module_name_repetitions)]
    #[must_use]
    pub const fn compute_timestamp_at_slot(&self, slot: u64) -> u64 {
        let slots_since_genesis = slot - self.genesis_slot;
        self.genesis_time + (slots_since_genesis * self.seconds_per_slot)
    }

    /// Returns the sync committee period at a given `epoch`.
    ///
    /// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/validator.md#sync-committee)
    #[must_use]
    pub const fn compute_sync_committee_period(&self, epoch: u64) -> u64 {
        epoch / self.epochs_per_sync_committee_period
    }

    /// Returns the sync committee period at a given `slot`.
    ///
    /// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#compute_sync_committee_period_at_slot)
    #[must_use]
    pub const fn compute_sync_committee_period_at_slot(&self, slot: u64) -> u64 {
        self.compute_sync_committee_period(self.compute_epoch_at_slot(slot))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_client_state() -> ClientState {
        ClientState {
            chain_id: 1,
            genesis_validators_root: B256::from_slice(&[1u8; 32]),
            min_sync_committee_participants: 10,
            genesis_time: 1_606_824_023, // Actual Ethereum mainnet genesis time
            genesis_slot: 0,
            fork_parameters: ForkParameters::default(),
            seconds_per_slot: 12, // Ethereum uses 12 seconds per slot
            slots_per_epoch: 32,  // Using 32 slots per epoch for tests
            epochs_per_sync_committee_period: 256, // Standard Ethereum value
            latest_slot: 12_000_000, // Recent slot value
            is_frozen: false,
            ibc_contract_address: Address::from_slice(&[2u8; 20]),
            ibc_commitment_slot: U256::from(3),
        }
    }

    #[test]
    fn test_compute_slot_at_timestamp() {
        let client_state = create_test_client_state();

        // Test case 1: Exactly at genesis time
        let slot_at_genesis = client_state.compute_slot_at_timestamp(client_state.genesis_time);
        assert_eq!(slot_at_genesis, Some(client_state.genesis_slot));

        // Test case 2: One slot after genesis
        let one_slot_later = client_state.genesis_time + client_state.seconds_per_slot;
        let slot = client_state.compute_slot_at_timestamp(one_slot_later);
        assert_eq!(slot, Some(client_state.genesis_slot + 1));

        // Test case 3: Multiple slots after genesis
        let multiple_slots_later = client_state.genesis_time + (client_state.seconds_per_slot * 10);
        let slot = client_state.compute_slot_at_timestamp(multiple_slots_later);
        assert_eq!(slot, Some(client_state.genesis_slot + 10));

        // Test case 4: Current time (approximation for March 2025)
        // March 2025 timestamp (approx): 1741305600
        let current_timestamp = 1_741_305_600;
        let expected_slots_since_genesis =
            (current_timestamp - client_state.genesis_time) / client_state.seconds_per_slot;
        let slot = client_state.compute_slot_at_timestamp(current_timestamp);
        assert_eq!(
            slot,
            Some(client_state.genesis_slot + expected_slots_since_genesis)
        );

        // Test case 5: Future time (approximation for 2030)
        // 2030 timestamp (approx): 1893456000
        let future_timestamp = 1_893_456_000;
        let expected_slots_since_genesis =
            (future_timestamp - client_state.genesis_time) / client_state.seconds_per_slot;
        let slot = client_state.compute_slot_at_timestamp(future_timestamp);
        assert_eq!(
            slot,
            Some(client_state.genesis_slot + expected_slots_since_genesis)
        );

        // Test case 6: Before genesis time (should return None)
        let before_genesis = client_state.genesis_time.saturating_sub(1);
        let slot = client_state.compute_slot_at_timestamp(before_genesis);
        assert_eq!(slot, None);

        // Testing with different seconds per slot
        let mut alternate_client_state = create_test_client_state();
        alternate_client_state.seconds_per_slot = 6; // Half the standard slot time

        // Test with alternate config - same timestamp should give twice the slot number
        let test_timestamp = alternate_client_state.genesis_time + 120; // 120 seconds after genesis
        let expected_slot =
            alternate_client_state.genesis_slot + (120 / alternate_client_state.seconds_per_slot);
        let slot = alternate_client_state.compute_slot_at_timestamp(test_timestamp);
        assert_eq!(slot, Some(expected_slot));

        // Testing with non-zero genesis slot
        let mut non_zero_genesis_state = create_test_client_state();
        non_zero_genesis_state.genesis_slot = 1_000_000; // Starting at slot 1M

        // Test with exactly genesis time - should give genesis slot
        let slot =
            non_zero_genesis_state.compute_slot_at_timestamp(non_zero_genesis_state.genesis_time);
        assert_eq!(slot, Some(non_zero_genesis_state.genesis_slot));

        // Test with time after genesis - slot number should be offset by genesis_slot
        let test_timestamp =
            non_zero_genesis_state.genesis_time + (non_zero_genesis_state.seconds_per_slot * 100);
        let expected_slot = non_zero_genesis_state.genesis_slot + 100;
        let slot = non_zero_genesis_state.compute_slot_at_timestamp(test_timestamp);
        assert_eq!(slot, Some(expected_slot));
    }

    #[test]
    fn test_compute_epoch_at_slot() {
        let client_state = create_test_client_state();

        // Test case 1: Slot 0 (first slot of epoch 0)
        let epoch = client_state.compute_epoch_at_slot(0);
        assert_eq!(epoch, 0);

        // Test case 2: Last slot of epoch 0
        let epoch = client_state.compute_epoch_at_slot(client_state.slots_per_epoch - 1);
        assert_eq!(epoch, 0);

        // Test case 3: First slot of epoch 1
        let epoch = client_state.compute_epoch_at_slot(client_state.slots_per_epoch);
        assert_eq!(epoch, 1);

        // Test case 4: Current mainnet slot (~11,000,000 as of early 2025)
        let current_slot = 11_000_000;
        let expected_epoch = current_slot / client_state.slots_per_epoch;
        let epoch = client_state.compute_epoch_at_slot(current_slot);
        assert_eq!(epoch, expected_epoch);

        // Test case 5: Future slot (2030 approximation - ~24,000,000)
        let future_slot = 24_000_000;
        let expected_epoch = future_slot / client_state.slots_per_epoch;
        let epoch = client_state.compute_epoch_at_slot(future_slot);
        assert_eq!(epoch, expected_epoch);

        // Test with alternate slots_per_epoch
        let mut alternate_client_state = create_test_client_state();
        alternate_client_state.slots_per_epoch = 64; // Double the standard value

        // Test with alternate config - same slot should give different epoch
        let test_slot = 320;
        let expected_epoch = test_slot / alternate_client_state.slots_per_epoch;
        let epoch = alternate_client_state.compute_epoch_at_slot(test_slot);
        assert_eq!(epoch, expected_epoch);
    }

    #[test]
    fn test_compute_timestamp_at_slot() {
        let client_state = create_test_client_state();

        // Test case 1: Genesis slot
        let timestamp = client_state.compute_timestamp_at_slot(client_state.genesis_slot);
        assert_eq!(timestamp, client_state.genesis_time);

        // Test case 2: One slot after genesis
        let timestamp = client_state.compute_timestamp_at_slot(client_state.genesis_slot + 1);
        assert_eq!(
            timestamp,
            client_state.genesis_time + client_state.seconds_per_slot
        );

        // Test case 3: Current mainnet slot (~11,000,000 as of early 2025)
        let current_slot = 11_000_000;
        let timestamp = client_state.compute_timestamp_at_slot(current_slot);
        let expected = client_state.genesis_time + (current_slot * client_state.seconds_per_slot);
        assert_eq!(timestamp, expected);

        // Test case 4: Future slot (2030 approximation - ~24,000,000)
        let future_slot = 24_000_000;
        let timestamp = client_state.compute_timestamp_at_slot(future_slot);
        let expected = client_state.genesis_time + (future_slot * client_state.seconds_per_slot);
        assert_eq!(timestamp, expected);

        // Testing with different seconds per slot
        let mut alternate_client_state = create_test_client_state();
        alternate_client_state.seconds_per_slot = 6; // Half the standard slot time

        // Test with alternate config - same slot should give timestamp with less time passed
        let test_slot = 100;
        let expected_time = alternate_client_state.genesis_time
            + (test_slot * alternate_client_state.seconds_per_slot);
        let timestamp = alternate_client_state.compute_timestamp_at_slot(test_slot);
        assert_eq!(timestamp, expected_time);

        // Testing with non-zero genesis slot
        let mut non_zero_genesis_state = create_test_client_state();
        non_zero_genesis_state.genesis_slot = 1_000_000; // Starting at slot 1M
        non_zero_genesis_state.genesis_time = 1_700_000_000; // Different genesis time

        // Test with exactly genesis slot - should give genesis time
        let timestamp =
            non_zero_genesis_state.compute_timestamp_at_slot(non_zero_genesis_state.genesis_slot);
        assert_eq!(timestamp, non_zero_genesis_state.genesis_time);

        // Test with slot after genesis
        let test_slot = non_zero_genesis_state.genesis_slot + 100;
        let expected_time =
            non_zero_genesis_state.genesis_time + (100 * non_zero_genesis_state.seconds_per_slot);
        let timestamp = non_zero_genesis_state.compute_timestamp_at_slot(test_slot);
        assert_eq!(timestamp, expected_time);

        // Note: The compute_timestamp_at_slot function will panic with an overflow
        // if given a slot < genesis_slot. This is acceptable behavior since the consensus
        // spec assumes slots are always >= genesis slot

        // Test with a different non-zero genesis time and slot
        let mut custom_genesis_state = create_test_client_state();
        custom_genesis_state.genesis_slot = 5_000_000; // Different genesis slot
        custom_genesis_state.genesis_time = 1_500_000_000; // Different genesis time
        custom_genesis_state.seconds_per_slot = 3; // Different seconds per slot

        // Test exactly at genesis
        let timestamp =
            custom_genesis_state.compute_timestamp_at_slot(custom_genesis_state.genesis_slot);
        assert_eq!(timestamp, custom_genesis_state.genesis_time);

        // Test at a later slot
        let later_slot = custom_genesis_state.genesis_slot + 5_000;
        let expected =
            custom_genesis_state.genesis_time + (5_000 * custom_genesis_state.seconds_per_slot);
        let timestamp = custom_genesis_state.compute_timestamp_at_slot(later_slot);
        assert_eq!(timestamp, expected);
    }

    #[test]
    fn test_compute_sync_committee_period() {
        let client_state = create_test_client_state();

        // Test case 1: Epoch 0
        let period = client_state.compute_sync_committee_period(0);
        assert_eq!(period, 0);

        // Test case 2: Last epoch of period 0
        let epoch = client_state.epochs_per_sync_committee_period - 1;
        let period = client_state.compute_sync_committee_period(epoch);
        assert_eq!(period, 0);

        // Test case 3: First epoch of period 1
        let epoch = client_state.epochs_per_sync_committee_period;
        let period = client_state.compute_sync_committee_period(epoch);
        assert_eq!(period, 1);

        // Test case 4: Current mainnet epoch (~343,750 as of early 2025)
        let current_epoch = 343_750;
        let expected_period = current_epoch / client_state.epochs_per_sync_committee_period;
        let period = client_state.compute_sync_committee_period(current_epoch);
        assert_eq!(period, expected_period);

        // Test case 5: Future epoch (~750,000 in 2030)
        let future_epoch = 750_000;
        let expected_period = future_epoch / client_state.epochs_per_sync_committee_period;
        let period = client_state.compute_sync_committee_period(future_epoch);
        assert_eq!(period, expected_period);

        // Test with alternate epochs_per_sync_committee_period
        let mut alternate_client_state = create_test_client_state();
        alternate_client_state.epochs_per_sync_committee_period = 128; // Half the standard value

        // Test with alternate config - same epoch should give different period
        let test_epoch = 512;
        let expected_period = test_epoch / alternate_client_state.epochs_per_sync_committee_period;
        let period = alternate_client_state.compute_sync_committee_period(test_epoch);
        assert_eq!(period, expected_period);
    }

    #[test]
    fn test_compute_sync_committee_period_at_slot() {
        let client_state = create_test_client_state();

        // Test case 1: Slot 0
        let period = client_state.compute_sync_committee_period_at_slot(0);
        assert_eq!(period, 0);

        // Test case 2: Last slot of period 0
        let slot = client_state.slots_per_epoch * client_state.epochs_per_sync_committee_period - 1;
        let period = client_state.compute_sync_committee_period_at_slot(slot);
        assert_eq!(period, 0);

        // Test case 3: First slot of period 1
        let slot = client_state.slots_per_epoch * client_state.epochs_per_sync_committee_period;
        let period = client_state.compute_sync_committee_period_at_slot(slot);
        assert_eq!(period, 1);

        // Test case 4: Current mainnet slot (~11,000,000 as of early 2025)
        let current_slot = 11_000_000;
        let current_epoch = current_slot / client_state.slots_per_epoch;
        let expected_period = current_epoch / client_state.epochs_per_sync_committee_period;
        let period = client_state.compute_sync_committee_period_at_slot(current_slot);
        assert_eq!(period, expected_period);

        // Test case 5: Future slot (2030 approximation - ~24,000,000)
        let future_slot = 24_000_000;
        let future_epoch = future_slot / client_state.slots_per_epoch;
        let expected_period = future_epoch / client_state.epochs_per_sync_committee_period;
        let period = client_state.compute_sync_committee_period_at_slot(future_slot);
        assert_eq!(period, expected_period);

        // Testing with a different configuration
        let mut alternate_client_state = create_test_client_state();
        alternate_client_state.slots_per_epoch = 64; // Twice as many slots per epoch
        alternate_client_state.epochs_per_sync_committee_period = 128; // Half as many epochs per period

        // Test alternate config: Last slot of period 0
        let slot = alternate_client_state.slots_per_epoch
            * alternate_client_state.epochs_per_sync_committee_period
            - 1;
        let period = alternate_client_state.compute_sync_committee_period_at_slot(slot);
        assert_eq!(period, 0);

        // Test alternate config: First slot of period 1
        let slot = alternate_client_state.slots_per_epoch
            * alternate_client_state.epochs_per_sync_committee_period;
        let period = alternate_client_state.compute_sync_committee_period_at_slot(slot);
        assert_eq!(period, 1);

        // Test alternate config: Same mainnet slot should give different period due to different params
        let current_slot = 11_000_000;
        let current_epoch = current_slot / alternate_client_state.slots_per_epoch;
        let expected_period =
            current_epoch / alternate_client_state.epochs_per_sync_committee_period;
        let period = alternate_client_state.compute_sync_committee_period_at_slot(current_slot);
        assert_eq!(period, expected_period);

        // Testing with non-zero genesis slot
        let mut non_zero_genesis_state = create_test_client_state();
        non_zero_genesis_state.genesis_slot = 1_000_000; // Starting at slot 1M

        // Test with exactly genesis slot
        let period = non_zero_genesis_state
            .compute_sync_committee_period_at_slot(non_zero_genesis_state.genesis_slot);
        let expected_epoch =
            non_zero_genesis_state.genesis_slot / non_zero_genesis_state.slots_per_epoch;
        let expected_period =
            expected_epoch / non_zero_genesis_state.epochs_per_sync_committee_period;
        assert_eq!(period, expected_period);

        // Test with slot after genesis
        let test_slot = non_zero_genesis_state.genesis_slot + 500_000;
        let period = non_zero_genesis_state.compute_sync_committee_period_at_slot(test_slot);
        let expected_epoch = test_slot / non_zero_genesis_state.slots_per_epoch;
        let expected_period =
            expected_epoch / non_zero_genesis_state.epochs_per_sync_committee_period;
        assert_eq!(period, expected_period);
    }
}
