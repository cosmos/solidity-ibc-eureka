//! This module defines [`ConsensusState`] and [`TrustedConsensusState`].

use alloy_primitives::{FixedBytes, B256};
use serde::{Deserialize, Serialize};

use crate::types::sync_committee::{ActiveSyncCommittee, SyncCommittee};

/// The consensus state of the Ethereum light client
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Clone, Default)]
pub struct ConsensusState {
    /// The slot number
    pub slot: u64,
    /// The state merkle root
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub state_root: B256,
    /// The storage merkle root
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub storage_root: B256,
    /// The timestamp of the consensus state
    // TODO: document the timestamp format (seconds since epoch?)
    pub timestamp: u64,
    /// aggregate public key of current sync committee
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub current_sync_committee: FixedBytes<48>,
    /// aggregate public key of next sync committee
    #[serde(with = "ethereum_utils::base64::option_with_default")]
    pub next_sync_committee: Option<FixedBytes<48>>,
}

/// The trusted consensus state of the Ethereum light client
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug, Default)]
#[allow(clippy::module_name_repetitions)]
pub struct TrustedConsensusState {
    /// The consensus state
    pub state: ConsensusState,
    /// Full sync committee data which corresponds to the aggregate key that we
    /// store at the client.
    ///
    /// This sync committee can either be the current sync committee or the next sync
    /// committee. That's because the verifier uses next or current sync committee's
    /// public keys to verify the signature against. It is based on
    pub sync_committee: ActiveSyncCommittee,
}

impl TrustedConsensusState {
    /// Returns the finalized slot of the trusted consensus state
    #[must_use]
    pub const fn finalized_slot(&self) -> u64 {
        self.state.slot
    }

    /// Returns the current slot of the trusted consensus state if it is available
    #[must_use]
    pub const fn current_sync_committee(&self) -> Option<&SyncCommittee> {
        if let ActiveSyncCommittee::Current(committee) = &self.sync_committee {
            Some(committee)
        } else {
            None
        }
    }

    /// Returns the next sync committee if it is available
    #[must_use]
    pub const fn next_sync_committee(&self) -> Option<&SyncCommittee> {
        if let ActiveSyncCommittee::Next(committee) = &self.sync_committee {
            Some(committee)
        } else {
            None
        }
    }
}
