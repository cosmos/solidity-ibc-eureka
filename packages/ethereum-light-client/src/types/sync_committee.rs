//! Types related to the sync committee

use alloy_primitives::Bytes;
use ethereum_utils::slot::compute_epoch_at_slot;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};
use tree_hash_derive::TreeHash;

use super::{
    bls::{BlsPublicKey, BlsSignature},
    height::Height,
    wrappers::WrappedVecBlsPublicKey,
};

/// The sync committee data
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default, TreeHash)]
pub struct SyncCommittee {
    /// The public keys of the sync committee
    pub pubkeys: WrappedVecBlsPublicKey,
    /// The aggregate public key of the sync committee
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub aggregate_pubkey: BlsPublicKey,
}

/// The active sync committee
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
#[allow(clippy::module_name_repetitions)]
pub enum ActiveSyncCommittee {
    /// The current sync committee
    Current(SyncCommittee),
    /// The next sync committee
    Next(SyncCommittee),
}

// TODO: should this actually return default at any point? If not, panic or error
impl Default for ActiveSyncCommittee {
    fn default() -> Self {
        Self::Current(SyncCommittee {
            pubkeys: WrappedVecBlsPublicKey::default(),
            aggregate_pubkey: BlsPublicKey::default(),
        })
    }
}

/// The trusted sync committee
// TODO: Could we use a enum to represent the current and next sync committee like
// `ActiveSyncCommittee`?
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
#[allow(clippy::module_name_repetitions)]
pub struct TrustedSyncCommittee {
    /// The trusted height
    pub trusted_height: Height,
    /// The current sync committee
    pub current_sync_committee: Option<SyncCommittee>,
    /// The next sync committee
    pub next_sync_committee: Option<SyncCommittee>,
}

impl TrustedSyncCommittee {
    /// Returns the active sync committee
    // TODO: should this actually return default at any point? If not, panic or error
    // also, if not returning default, remove the impl Default
    #[must_use]
    pub fn get_active_sync_committee(&self) -> ActiveSyncCommittee {
        match (&self.current_sync_committee, &self.next_sync_committee) {
            (Some(sync_committee), _) => ActiveSyncCommittee::Current(sync_committee.clone()),
            (_, Some(sync_committee)) => ActiveSyncCommittee::Next(sync_committee.clone()),
            _ => ActiveSyncCommittee::default(),
        }
    }
}

/// The sync committee aggregate
#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
pub struct SyncAggregate {
    /// The bits representing the sync committee's participation.
    #[serde_as(as = "Base64")]
    pub sync_committee_bits: Bytes, // TODO: Consider changing this to a BitVector
    /// The aggregated signature of the sync committee.
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub sync_committee_signature: BlsSignature,
}

impl SyncAggregate {
    // TODO: Unit test
    /// Returns the number of bits that are set to `true`.
    #[must_use]
    pub fn num_sync_committe_participants(&self) -> usize {
        self.sync_committee_bits
            .iter()
            .map(|byte| byte.count_ones())
            .sum::<u32>() as usize
    }

    /// Returns the size of the sync committee.
    pub fn sync_committee_size(&self) -> usize {
        self.sync_committee_bits.len() * 8
    }

    /// Returns if at least 2/3 of the sync committee signed
    ///
    /// <https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#process_light_client_update>
    // TODO: Unit test
    pub fn validate_signature_supermajority(&self) -> bool {
        self.num_sync_committe_participants() * 3 >= self.sync_committee_size() * 2
    }
}

/// Returns the sync committee period at a given `epoch`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/validator.md#sync-committee)
#[must_use]
pub const fn compute_sync_committee_period(
    epochs_per_sync_committee_period: u64,
    epoch: u64,
) -> u64 {
    epoch / epochs_per_sync_committee_period
}

/// Returns the sync committee period at a given `slot`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#compute_sync_committee_period_at_slot)
#[must_use]
pub fn compute_sync_committee_period_at_slot(
    slots_per_epoch: u64,
    epochs_per_sync_committee_period: u64,
    slot: u64,
) -> u64 {
    compute_sync_committee_period(
        epochs_per_sync_committee_period,
        compute_epoch_at_slot(slots_per_epoch, slot),
    )
}

#[cfg(test)]
#[allow(clippy::pedantic)]
mod test {
    use crate::test::fixture_types::UpdateClient;
    use crate::types::sync_committee::{SyncAggregate, SyncCommittee};

    use alloy_primitives::{hex::FromHex, B256};
    use alloy_rpc_types_beacon::BlsSignature;
    use ethereum_test_utils::fixtures;
    use tree_hash::TreeHash;

    #[test]
    fn test_sync_committee_tree_hash_root() {
        let sync_committee: SyncCommittee = fixtures::load("sync_committee_fixture");
        assert_ne!(sync_committee, SyncCommittee::default());

        let actual_tree_hash_root = sync_committee.tree_hash_root();
        let expected_tree_hash_root =
            B256::from_hex("0x5361eb179f7499edbf09e514d317002f1d365d72e14a56c931e9edaccca3ff29")
                .unwrap();

        assert_eq!(expected_tree_hash_root, actual_tree_hash_root);
    }

    #[test]
    fn test_validate_signature_supermajority() {
        // not supermajority
        let sync_aggregate = SyncAggregate {
            sync_committee_bits: vec![0b10001001].into(),
            sync_committee_signature: BlsSignature::default(),
        };
        assert_eq!(sync_aggregate.num_sync_committe_participants(), 3);
        assert_eq!(sync_aggregate.sync_committee_size(), 8);
        assert!(!sync_aggregate.validate_signature_supermajority());

        // not supermajority
        let sync_aggregate = SyncAggregate {
            sync_committee_bits: vec![0b10000001, 0b11111111, 0b00010000, 0b00000000].into(),
            sync_committee_signature: BlsSignature::default(),
        };
        assert_eq!(sync_aggregate.num_sync_committe_participants(), 11);
        assert_eq!(sync_aggregate.sync_committee_size(), 32);
        assert!(!sync_aggregate.validate_signature_supermajority());

        // not supermajority
        let sync_aggregate = SyncAggregate {
            sync_committee_bits: vec![0b11101001, 0b11111111, 0b01010000, 0b01111110].into(),
            sync_committee_signature: BlsSignature::default(),
        };
        assert_eq!(sync_aggregate.num_sync_committe_participants(), 21);
        assert_eq!(sync_aggregate.sync_committee_size(), 32);
        assert!(!sync_aggregate.validate_signature_supermajority());

        // supermajority
        let sync_aggregate = SyncAggregate {
            sync_committee_bits: vec![0b11101001, 0b11111111, 0b01011000, 0b01111110].into(),
            sync_committee_signature: BlsSignature::default(),
        };
        assert_eq!(sync_aggregate.num_sync_committe_participants(), 22);
        assert_eq!(sync_aggregate.sync_committee_size(), 32);
        assert!(sync_aggregate.validate_signature_supermajority());

        // valid sync aggregate from fixtures with supermajority
        let fixture: fixtures::StepFixture =
            fixtures::load("TestICS20TransferNativeCosmosCoinsToEthereumAndBack_Groth16");
        let client_update: UpdateClient = fixture.get_data_at_step(1);
        let sync_aggregate = client_update.updates[0]
            .consensus_update
            .sync_aggregate
            .clone();
        assert!(sync_aggregate.validate_signature_supermajority());
    }
}
