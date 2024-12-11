//! This module defines the types used in the light client updates

use alloy_primitives::{Address, Bytes, B256, U256};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tree_hash_derive::TreeHash;

use crate::config::consts::{
    EXECUTION_BRANCH_DEPTH, FINALITY_BRANCH_DEPTH, NEXT_SYNC_COMMITTEE_BRANCH_DEPTH,
};

use super::{
    sync_committee::{SyncAggregate, SyncCommittee, TrustedSyncCommittee},
    wrappers::{WrappedBloom, WrappedBranch, WrappedBytes},
};

/// The header of a light client update
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct Header {
    /// The trusted sync committee
    pub trusted_sync_committee: TrustedSyncCommittee,
    /// The consensus update
    pub consensus_update: LightClientUpdate,
    /// The account update
    pub account_update: AccountUpdate,
}

/// A light client update
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
#[allow(clippy::module_name_repetitions)]
pub struct LightClientUpdate {
    /// Header attested to by the sync committee
    pub attested_header: LightClientHeader,
    /// Next sync committee corresponding to `attested_header.state_root`
    #[serde(default)] // TODO: Check if this can be removed in #143
    pub next_sync_committee: Option<SyncCommittee>,
    /// The branch of the next sync committee
    pub next_sync_committee_branch: Option<WrappedBranch<NEXT_SYNC_COMMITTEE_BRANCH_DEPTH>>,
    /// Finalized header corresponding to `attested_header.state_root`
    pub finalized_header: LightClientHeader,
    /// Branch of the finalized header
    pub finality_branch: WrappedBranch<FINALITY_BRANCH_DEPTH>,
    /// Sync committee aggregate signature
    pub sync_aggregate: SyncAggregate,
    /// Slot at which the aggregate signature was created (untrusted)
    pub signature_slot: u64,
}

/// The account update
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct AccountUpdate {
    /// The account proof
    pub account_proof: AccountProof,
}

/// The account proof
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default)]
pub struct AccountProof {
    /// The account storage root
    #[schemars(with = "String")]
    pub storage_root: B256,
    /// The account proof
    #[schemars(with = "Vec<String>")]
    pub proof: Vec<Bytes>,
}

/// The header of a light client
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default, TreeHash)]
#[allow(clippy::module_name_repetitions)]
pub struct LightClientHeader {
    /// The beacon block header
    pub beacon: BeaconBlockHeader,
    /// The execution payload header
    pub execution: ExecutionPayloadHeader,
    /// The execution branch
    pub execution_branch: WrappedBranch<EXECUTION_BRANCH_DEPTH>,
}

/// The beacon block header
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default, TreeHash)]
pub struct BeaconBlockHeader {
    /// The slot to which this block corresponds
    pub slot: u64,
    /// The index of validator in validator registry
    pub proposer_index: u64,
    /// The signing merkle root of the parent `BeaconBlock`
    #[schemars(with = "String")]
    pub parent_root: B256,
    /// The tree hash merkle root of the `BeaconState` for the `BeaconBlock`
    #[schemars(with = "String")]
    pub state_root: B256,
    /// The tree hash merkle root of the `BeaconBlockBody` for the `BeaconBlock`
    #[schemars(with = "String")]
    pub body_root: B256,
}

/// Header to track the execution block
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug, Default, TreeHash)]
pub struct ExecutionPayloadHeader {
    /// The parent hash of the execution payload header
    #[schemars(with = "String")]
    pub parent_hash: B256,
    /// Block fee recipient
    #[schemars(with = "String")]
    pub fee_recipient: Address,
    /// The state root
    #[schemars(with = "String")]
    pub state_root: B256,
    /// The root of the receipts trie
    #[schemars(with = "String")]
    pub receipts_root: B256,
    /// The logs bloom filter
    #[schemars(with = "Vec<String>")]
    pub logs_bloom: WrappedBloom,
    /// The previous Randao value, used to compute the randomness on the execution layer.
    #[schemars(with = "String")]
    pub prev_randao: B256,
    /// The block number of the execution payload
    pub block_number: u64,
    /// Execution block gas limit
    pub gas_limit: u64,
    /// Execution block gas used
    #[serde(default)]
    pub gas_used: u64,
    /// The timestamp of the execution payload
    pub timestamp: u64,
    /// The extra data of the execution payload
    #[schemars(with = "String")]
    pub extra_data: WrappedBytes,
    /// Block base fee per gas
    #[schemars(with = "String")]
    pub base_fee_per_gas: U256,
    /// The block hash
    #[schemars(with = "String")]
    pub block_hash: B256,
    /// SSZ hash tree root of the transaction list
    #[schemars(with = "String")]
    pub transactions_root: B256,
    /// Tree root of the withdrawals list
    #[schemars(with = "String")]
    pub withdrawals_root: B256,
    /// Blob gas used (new in Deneb)
    #[serde(default)]
    pub blob_gas_used: u64,
    /// Excess blob gas (new in Deneb)
    #[serde(default)]
    pub excess_blob_gas: u64,
}
