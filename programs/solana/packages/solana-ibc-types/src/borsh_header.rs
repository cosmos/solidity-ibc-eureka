//! Borsh-serializable wrapper types for Tendermint Header
//!
//! These types mirror ibc-client-tendermint::types::Header and related types,
//! but use Borsh serialization for efficient memory usage on Solana.
//!
//! Memory comparison:
//! - Protobuf: 38KB serialized → ~300KB deserialized
//! - Borsh: ~38KB serialized → ~60-90KB deserialized
//!
//! This allows fitting mainnet-sized headers within Solana's 256KB heap limit.
//!
//! Note: Conversion functions (From/TryFrom implementations) are implemented
//! in the packages that have access to ibc-rs types (relayer and ics07-tendermint).

use borsh::{BorshDeserialize, BorshSerialize};

/// Borsh-serializable wrapper for ibc_client_tendermint::types::Header
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshHeader {
    pub signed_header: BorshSignedHeader,
    pub validator_set: BorshValidatorSet,
    pub trusted_height: BorshHeight,
    pub trusted_next_validator_set: BorshValidatorSet,
}

/// Borsh-serializable wrapper for tendermint::block::signed_header::SignedHeader
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshSignedHeader {
    pub header: BorshBlockHeader,
    pub commit: BorshCommit,
}

/// Borsh-serializable wrapper for tendermint::block::Header
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshBlockHeader {
    pub version: BorshConsensusVersion,
    pub chain_id: String,
    pub height: u64,
    pub time: BorshTimestamp,
    pub last_block_id: Option<BorshBlockId>,
    pub last_commit_hash: Option<Vec<u8>>,
    pub data_hash: Option<Vec<u8>>,
    pub validators_hash: Vec<u8>,
    pub next_validators_hash: Vec<u8>,
    pub consensus_hash: Vec<u8>,
    pub app_hash: Vec<u8>,
    pub last_results_hash: Option<Vec<u8>>,
    pub evidence_hash: Option<Vec<u8>>,
    pub proposer_address: Vec<u8>,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshConsensusVersion {
    pub block: u64,
    pub app: u64,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshTimestamp {
    pub secs: i64,
    pub nanos: i32,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshBlockId {
    pub hash: Vec<u8>,
    pub part_set_header: BorshPartSetHeader,
}

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshPartSetHeader {
    pub total: u32,
    pub hash: Vec<u8>,
}

/// Borsh-serializable wrapper for tendermint::block::Commit
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshCommit {
    pub height: u64,
    pub round: u16,
    pub block_id: BorshBlockId,
    pub signatures: Vec<BorshCommitSig>,
}

/// Borsh-serializable wrapper for tendermint::block::CommitSig
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum BorshCommitSig {
    BlockIdFlagAbsent,
    BlockIdFlagCommit {
        validator_address: Vec<u8>,
        timestamp: BorshTimestamp,
        signature: Vec<u8>,
    },
    BlockIdFlagNil {
        validator_address: Vec<u8>,
        timestamp: BorshTimestamp,
        signature: Vec<u8>,
    },
}

/// Borsh-serializable wrapper for tendermint::validator::Set
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshValidatorSet {
    pub validators: Vec<BorshValidator>,
    pub proposer: Option<BorshValidator>,
    pub total_voting_power: u64,
}

/// Borsh-serializable wrapper for tendermint::validator::Info
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshValidator {
    pub address: Vec<u8>,
    pub pub_key: BorshPublicKey,
    pub voting_power: u64,
    pub proposer_priority: i64,
}

/// Borsh-serializable wrapper for tendermint::public_key::PublicKey
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum BorshPublicKey {
    Ed25519(Vec<u8>),
    Secp256k1(Vec<u8>),
}

/// Borsh-serializable wrapper for ibc_core_client_types::Height
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct BorshHeight {
    pub revision_number: u64,
    pub revision_height: u64,
}
