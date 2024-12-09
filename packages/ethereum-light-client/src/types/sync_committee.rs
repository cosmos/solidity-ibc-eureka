use alloy_primitives::Bytes;
use ethereum_utils::slot::compute_epoch_at_slot;
use serde::{Deserialize, Serialize};
use tree_hash_derive::TreeHash;

use super::{
    bls::{BlsPublicKey, BlsSignature},
    height::Height,
    wrappers::WrappedVecBlsPublicKey,
};

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default, TreeHash)]
pub struct SyncCommittee {
    pub pubkeys: WrappedVecBlsPublicKey,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub aggregate_pubkey: BlsPublicKey,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub enum ActiveSyncCommittee {
    Current(SyncCommittee),
    Next(SyncCommittee),
}

impl Default for ActiveSyncCommittee {
    fn default() -> Self {
        ActiveSyncCommittee::Current(SyncCommittee {
            pubkeys: WrappedVecBlsPublicKey::default(),
            aggregate_pubkey: BlsPublicKey::default(),
        })
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct TrustedSyncCommittee {
    pub trusted_height: Height,
    pub current_sync_committee: Option<SyncCommittee>,
    pub next_sync_committee: Option<SyncCommittee>,
}

impl TrustedSyncCommittee {
    pub fn get_active_sync_committee(&self) -> ActiveSyncCommittee {
        if let Some(sync_committee) = &self.current_sync_committee {
            ActiveSyncCommittee::Current(sync_committee.clone())
        } else if let Some(sync_committee) = &self.next_sync_committee {
            ActiveSyncCommittee::Next(sync_committee.clone())
        } else {
            ActiveSyncCommittee::default()
        }
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct SyncAggregate {
    /// The bits representing the sync committee's participation.
    #[serde(with = "ethereum_utils::base64")]
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

    // TODO: Unit test
    // Returns if at least 2/3 of the sync committee signed
    //
    // https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#process_light_client_update
    pub fn validate_signature_supermajority(&self) -> bool {
        self.num_sync_committe_participants() * 3 >= self.sync_committee_bits.len() * 8 * 2
    }
}

/// Returns the sync committee period at a given `epoch`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/validator.md#sync-committee)
pub fn compute_sync_committee_period(epochs_per_sync_committee_period: u64, epoch: u64) -> u64 {
    epoch / epochs_per_sync_committee_period
}

/// Returns the sync committee period at a given `slot`.
///
/// [See in consensus-spec](https://github.com/ethereum/consensus-specs/blob/dev/specs/altair/light-client/sync-protocol.md#compute_sync_committee_period_at_slot)
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
mod test {
    use crate::types::sync_committee::SyncCommittee;

    use alloy_primitives::{hex::FromHex, B256};
    use ethereum_test_utils::fixtures::load_fixture;
    use tree_hash::TreeHash;

    #[test]
    fn test_sync_committee_tree_hash_root() {
        let sync_committee: SyncCommittee = load_fixture("sync_committee_fixture");
        assert_ne!(sync_committee, SyncCommittee::default());

        let actual_tree_hash_root = sync_committee.tree_hash_root();
        let expected_tree_hash_root =
            B256::from_hex("0x5361eb179f7499edbf09e514d317002f1d365d72e14a56c931e9edaccca3ff29")
                .unwrap();

        assert_eq!(expected_tree_hash_root, actual_tree_hash_root);
    }
}
