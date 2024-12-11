//! This module defines [`TrieDBError`].

use ethereum_utils::hex;

#[derive(Debug, PartialEq, Eq, thiserror::Error, Clone)]
#[allow(missing_docs, clippy::module_name_repetitions)]
pub enum TrieDBError {
    #[error("get trie node failed: {0}")]
    GetTrieNodeFailed(String),

    #[error("rlp decoding failed: {0:?}")]
    RlpDecode(#[from] rlp::DecoderError),

    #[error(
        "proof is invalid due to value mismatch, expected: {expected}, actual: {actual}",
        expected = hex::to_hex(expected),
        actual = hex::to_hex(actual)
    )]
    ValueMismatch { expected: Vec<u8>, actual: Vec<u8> },

    #[error("proof is invalid due to missing value: {v}", v = hex::to_hex(value))]
    ValueMissing { value: Vec<u8> },
}
