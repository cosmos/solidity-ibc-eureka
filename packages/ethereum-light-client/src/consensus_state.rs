use alloy_primitives::{FixedBytes, B256};
use serde::{Deserialize, Serialize};

use crate::types::sync_committee::{ActiveSyncCommittee, SyncCommittee};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone, Default)]
pub struct ConsensusState {
    pub slot: u64,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub state_root: B256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub storage_root: B256,
    pub timestamp: u64,
    /// aggregate public key of current sync committee
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub current_sync_committee: FixedBytes<48>,
    /// aggregate public key of next sync committee
    #[serde(with = "ethereum_utils::base64::option_with_default")]
    pub next_sync_committee: Option<FixedBytes<48>>,
}

impl From<Vec<u8>> for ConsensusState {
    fn from(value: Vec<u8>) -> Self {
        serde_json::from_slice(&value).unwrap()
    }
}

impl From<ConsensusState> for Vec<u8> {
    fn from(value: ConsensusState) -> Self {
        serde_json::to_vec(&value).unwrap()
    }
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct TrustedConsensusState {
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
    pub fn finalized_slot(&self) -> u64 {
        self.state.slot
    }

    pub fn current_sync_committee(&self) -> Option<&SyncCommittee> {
        if let ActiveSyncCommittee::Current(committee) = &self.sync_committee {
            Some(committee)
        } else {
            None
        }
    }

    pub fn next_sync_committee(&self) -> Option<&SyncCommittee> {
        if let ActiveSyncCommittee::Next(committee) = &self.sync_committee {
            Some(committee)
        } else {
            None
        }
    }
}
