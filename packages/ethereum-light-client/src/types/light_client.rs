use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use tree_hash_derive::TreeHash;

use crate::config::consts::{
    EXECUTION_PAYLOAD_INDEX, FINALIZED_ROOT_INDEX, NEXT_SYNC_COMMITTEE_INDEX,
};

use super::{
    sync_committee::{SyncAggregate, SyncCommittee, TrustedSyncCommittee},
    wrappers::{WrappedBloom, WrappedBranch, WrappedBytes},
};

pub const EXECUTION_BRANCH_DEPTH: usize = EXECUTION_PAYLOAD_INDEX.ilog2() as usize;
pub const NEXT_SYNC_COMMITTEE_BRANCH_DEPTH: usize = NEXT_SYNC_COMMITTEE_INDEX.ilog2() as usize;
pub const FINALITY_BRANCH_DEPTH: usize = FINALIZED_ROOT_INDEX.ilog2() as usize;

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct Header {
    pub trusted_sync_committee: TrustedSyncCommittee,
    pub consensus_update: LightClientUpdate,
    pub account_update: AccountUpdate,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct LightClientUpdate {
    /// Header attested to by the sync committee
    pub attested_header: LightClientHeader,
    /// Next sync committee corresponding to `attested_header.state_root`
    #[serde(default)] // TODO: Check if this can be removed in #143
    pub next_sync_committee: Option<SyncCommittee>,
    pub next_sync_committee_branch: Option<WrappedBranch<NEXT_SYNC_COMMITTEE_BRANCH_DEPTH>>,
    /// Finalized header corresponding to `attested_header.state_root`
    pub finalized_header: LightClientHeader,
    pub finality_branch: WrappedBranch<FINALITY_BRANCH_DEPTH>,
    /// Sync committee aggregate signature
    pub sync_aggregate: SyncAggregate,
    /// Slot at which the aggregate signature was created (untrusted)
    pub signature_slot: u64,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct AccountUpdate {
    pub account_proof: AccountProof,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default)]
pub struct AccountProof {
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub storage_root: B256,
    pub proof: Vec<WrappedBytes>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default, TreeHash)]
pub struct LightClientHeader {
    pub beacon: BeaconBlockHeader,
    pub execution: ExecutionPayloadHeader,
    pub execution_branch: WrappedBranch<EXECUTION_BRANCH_DEPTH>,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default, TreeHash)]
pub struct BeaconBlockHeader {
    pub slot: u64,
    pub proposer_index: u64,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub parent_root: B256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub state_root: B256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub body_root: B256,
}

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug, Default, TreeHash)]
pub struct ExecutionPayloadHeader {
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub parent_hash: B256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub fee_recipient: Address,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub state_root: B256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub receipts_root: B256,
    pub logs_bloom: WrappedBloom,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub prev_randao: B256,
    pub block_number: u64,
    pub gas_limit: u64,
    #[serde(default)]
    pub gas_used: u64,
    pub timestamp: u64,
    pub extra_data: WrappedBytes,
    #[serde(with = "ethereum_utils::base64::uint256")]
    pub base_fee_per_gas: U256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub block_hash: B256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub transactions_root: B256,
    #[serde(with = "ethereum_utils::base64::fixed_size")]
    pub withdrawals_root: B256,
    // new in Deneb
    #[serde(default)]
    pub blob_gas_used: u64,
    // new in Deneb
    #[serde(default)]
    pub excess_blob_gas: u64,
}
