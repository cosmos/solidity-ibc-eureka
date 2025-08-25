//! This module defines [`EthereumIBCError`].

use alloy_primitives::B256;
use ethereum_types::consensus::bls::BlsPublicKey;

/// Error types for Ethereum IBC light client operations
#[derive(thiserror::Error, Debug, Clone, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub enum EthereumIBCError {
    /// Invalid path length error
    #[error("invalid path length, expected {expected} but found {found}")]
    InvalidPathLength {
        /// Expected length
        expected: usize,
        /// Found length
        found: usize,
    },

    /// Unable to decode storage proof
    #[error("unable to decode storage proof")]
    StorageProofDecode,

    /// Invalid commitment key error
    #[error("invalid commitment key, expected ({0}) but found ({1})")]
    InvalidCommitmentKey(String, String),

    /// Stored value mismatch error
    #[error("expected value ({expected}) and stored value ({actual}) don't match", 
        expected = hex::encode(expected),
        actual = hex::encode(actual)
    )]
    StoredValueMistmatch {
        /// Expected value
        expected: Vec<u8>,
        /// Actual value
        actual: Vec<u8>,
    },

    /// Verify storage proof error
    #[error("verify storage proof error: {0}")]
    VerifyStorageProof(String),

    /// Insufficient number of sync committee participants
    #[error("insufficient number of sync committee participants ({0})")]
    InsufficientSyncCommitteeParticipants(u64),

    /// Insufficient sync committee length error
    #[error("insufficient number of sync committee addresses ({found}) but expected ({expected})")]
    InsufficientSyncCommitteeLength {
        /// Expected count
        expected: u64,
        /// Found count
        found: u64,
    },

    /// Must be Electra fork or later
    #[error("unsupported fork version, must be electra or later")]
    MustBeElectraOrLater,

    /// Invalid merkle branch error
    #[error(transparent)]
    InvalidMerkleBranch(#[from] Box<InvalidMerkleBranch>), // boxed to decrease enum size

    /// Invalid normalized merkle branch error
    #[error(
        "invalid normalized merkle branch, expected {num_extra} empty bytes in {normalized_branch}",
        normalized_branch = .normalized_branch.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
    )]
    InvalidNormalizedMerkleBranch {
        /// Number of extra bytes
        num_extra: usize,
        /// Normalized branch
        normalized_branch: Vec<B256>,
    },

    /// Finalized slot cannot be the genesis slot
    #[error("finalized slot cannot be the genesis slot")]
    FinalizedSlotIsGenesis,

    /// Update signature slot is more recent than current slot
    #[error(
        "update signature slot {update_signature_slot} is more recent than the \
        calculated current slot {current_slot}"
    )]
    UpdateSignatureSlotMoreRecentThanCurrentSlot {
        /// Current slot
        current_slot: u64,
        /// Update signature slot
        update_signature_slot: u64,
    },

    /// Invalid slot ordering error
    #[error(
        "(update_signature_slot > update_attested_slot >= update_finalized_slot) must hold, \
        found: ({update_signature_slot} > {update_attested_slot} >= {update_finalized_slot})"
    )]
    InvalidSlots {
        /// Update signature slot
        update_signature_slot: u64,
        /// Update attested slot
        update_attested_slot: u64,
        /// Update finalized slot
        update_finalized_slot: u64,
    },

    /// Invalid signature period when next sync committee exists
    #[error(
        "signature period ({signature_period}) must be equal to `store_period` \
        ({stored_period}) or `store_period + 1` when the next sync committee is stored"
    )]
    InvalidSignaturePeriodWhenNextSyncCommitteeExists {
        /// Signature period
        signature_period: u64,
        /// Stored period
        stored_period: u64,
    },

    /// Invalid signature period when next sync committee does not exist
    #[error(
        "signature period ({signature_period}) must be equal to `store_period` \
        ({stored_period}) when the next sync committee is not stored"
    )]
    InvalidSignaturePeriodWhenNextSyncCommitteeDoesNotExist {
        /// Signature period
        signature_period: u64,
        /// Stored period
        stored_period: u64,
    },

    /// Irrelevant update error
    #[error(
        "irrelevant update since the order of the slots in the update data, and stored data is not correct. \
        either the update_attested_slot (found {update_attested_slot}) must be > the trusted_finalized_slot \
        (found {trusted_finalized_slot}) or if it is not, then the update_attested_period \
        (found {update_attested_period}) must be the same as the store_period (found {stored_period}) and \
        the update_sync_committee must be set (was set: {update_sync_committee_is_set}) and the trusted \
        next_sync_committee must be unset (was set: {trusted_next_sync_committee_is_set})"
    )]
    IrrelevantUpdate {
        /// Update attested slot
        update_attested_slot: u64,
        /// Trusted finalized slot
        trusted_finalized_slot: u64,
        /// Update attested period
        update_attested_period: u64,
        /// Stored period
        stored_period: u64,
        /// Whether update sync committee is set
        update_sync_committee_is_set: bool,
        /// Whether trusted next sync committee is set
        trusted_next_sync_committee_is_set: bool,
    },

    /// Next sync committee mismatch error
    #[error(
        "next sync committee ({found}) does not match with the one in the current state ({expected})"
    )]
    NextSyncCommitteeMismatch {
        /// Expected public key
        expected: BlsPublicKey,
        /// Found public key
        found: BlsPublicKey,
    },

    /// Current sync committee mismatch error
    #[error(
        "current sync committee ({found}) does not match with the one in the current state ({expected})"
    )]
    CurrenttSyncCommitteeMismatch {
        /// Expected public key
        expected: BlsPublicKey,
        /// Found public key
        found: BlsPublicKey,
    },

    /// Aggregate public key mismatch error
    #[error("aggregate public key mismatch: expected {expected} but found {found}")]
    AggregatePubkeyMismatch {
        /// Expected public key
        expected: BlsPublicKey,
        /// Found public key
        found: BlsPublicKey,
    },

    /// Expected current sync committee error
    #[error(
        "expected current sync committee to be provided since `update_period == current_period`"
    )]
    ExpectedCurrentSyncCommittee,

    /// Expected next sync committee error
    #[error("expected next sync committee to be provided for signature verification`")]
    ExpectedNextSyncCommittee,

    /// Expected next sync committee update error
    #[error("expected next sync committee to be provided in the update since `update_period > current_period`")]
    ExpectedNextSyncCommitteeUpdate,

    /// Next sync committee unknown error
    #[error("expected next sync committee to be known and stored in state")]
    NextSyncCommitteeUnknown,

    /// Unexpected next sync committee error
    #[error("unexpected next sync committee in the update")]
    UnexpectedNextSyncCommittee,

    /// BLS aggregate error
    #[error("bls aggregate error: {0}")]
    BlsAggregateError(String),

    /// Fast aggregate verify error
    #[error("fast aggregate verify error: {0}")]
    FastAggregateVerifyError(String),

    /// Not enough signatures error
    #[error("not enough signatures")]
    NotEnoughSignatures,

    /// Failed to validate finalized header
    #[error("failed to verify finalized_header is finalized: {0}")]
    ValidateFinalizedHeaderFailed(#[source] Box<EthereumIBCError>),

    /// Failed to validate next sync committee
    #[error("failed to verify next sync committee against attested header: {0}")]
    ValidateNextSyncCommitteeFailed(#[source] Box<EthereumIBCError>),

    /// Store period must be equal to finalized period
    #[error("client's store period must be equal to update's finalized period")]
    StorePeriodMustBeEqualToFinalizedPeriod,

    /// Failed to compute slot at timestamp
    #[error("failed to compute slot at timestamp with  \
        (timestamp ({timestamp}) - genesis ({genesis})) / seconds_per_slot ({seconds_per_slot}) + genesis_slot ({genesis_slot})"
    )]
    FailedToComputeSlotAtTimestamp {
        /// Timestamp value
        timestamp: u64,
        /// Genesis timestamp
        genesis: u64,
        /// Seconds per slot
        seconds_per_slot: u64,
        /// Genesis slot
        genesis_slot: u64,
    },

    /// Misbehaviour slot mismatch error
    #[error("conflicting updates are for different slots: {0} != {1}")]
    MisbehaviourSlotMismatch(u64, u64),

    /// Misbehaviour storage roots match error
    #[error("storage roots are not conflicting: {0} == {0}")]
    MisbehaviourStorageRootsMatch(B256),

    /// Invalid update slot error
    #[error(
        "update must be against a previous consensus state: \
        stored consensus state slot: {consensus_state_slot}, \
        update finalized header slot: {update_finalized_slot}"
    )]
    InvalidUpdateSlot {
        /// Consensus state slot
        consensus_state_slot: u64,
        /// Update finalized slot
        update_finalized_slot: u64,
    },

    /// Client and consensus slot mismatch error
    #[error(
        "client and consensus slot mismatch: \
        client state slot: {client_state_slot}, \
        consensus state slot: {consensus_state_slot}"
    )]
    ClientAndConsensusSlotMismatch {
        /// Client state slot
        client_state_slot: u64,
        /// Consensus state slot
        consensus_state_slot: u64,
    },
}

#[derive(Debug, PartialEq, Eq, Clone, thiserror::Error)]
#[error("invalid merkle branch \
    (leaf: {leaf}, branch: [{branch}], \
    depth: {depth}, index: {index}, root: {root}, found: {found})",
    branch = .branch.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
)]
/// Error details for invalid Merkle branch verification
pub struct InvalidMerkleBranch {
    /// Leaf hash
    pub leaf: B256,
    /// Branch hashes
    pub branch: Vec<B256>,
    /// Tree depth
    pub depth: usize,
    /// Leaf index
    pub index: u64,
    /// Expected root hash
    pub root: B256,
    /// Computed root hash
    pub found: B256,
}

impl EthereumIBCError {
    /// Constructs an [`EthereumIBCError::InvalidMerkleBranch`] variant.
    #[must_use]
    pub fn invalid_merkle_branch(
        leaf: B256,
        branch: Vec<B256>,
        depth: usize,
        index: u64,
        root: B256,
        found: B256,
    ) -> Self {
        Self::InvalidMerkleBranch(Box::new(InvalidMerkleBranch {
            leaf,
            branch,
            depth,
            index,
            root,
            found,
        }))
    }
}
