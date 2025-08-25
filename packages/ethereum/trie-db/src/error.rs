//! This module defines [`TrieDBError`].

/// Error types for trie database operations
#[derive(Debug, PartialEq, Eq, thiserror::Error, Clone)]
#[allow(clippy::module_name_repetitions)]
pub enum TrieDBError {
    /// Failed to retrieve a trie node
    #[error("get trie node failed: {0}")]
    GetTrieNodeFailed(String),

    /// RLP decoding failed
    #[error("rlp decoding failed: {0:?}")]
    RlpDecode(#[from] rlp::DecoderError),

    /// Value mismatch in proof verification
    #[error(
        "proof is invalid due to value mismatch, expected: {expected}, actual: {actual}",
        expected = hex::encode(expected),
        actual = hex::encode(actual)
    )]
    ValueMismatch {
        /// The expected value
        expected: Vec<u8>,
        /// The actual value found
        actual: Vec<u8>,
    },

    /// Expected value is missing from the trie
    #[error("proof is invalid due to missing value: {v}", v = hex::encode(value))]
    ValueMissing {
        /// The value that was expected to be present
        value: Vec<u8>,
    },

    /// Value should not exist in the trie but was found
    #[error("proof is invalid due to unexpected value: {v}", v = hex::encode(value))]
    ValueShouldBeMissing {
        /// The value that should not have been present
        value: Vec<u8>,
    },
}
