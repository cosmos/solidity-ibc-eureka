use crate::types::ConsensusState;
use anchor_lang::prelude::*;
pub use solana_ibc_constants::CHUNK_DATA_SIZE;

/// On-chain PDA storing a Tendermint consensus state for a specific block height.
///
/// Contains the block timestamp, Merkle root and next-validators hash.
/// The ICS26 router reads these accounts to verify IBC packet commitment
/// proofs against the confirmed Tendermint state. One account exists per
/// verified height, derived from `["consensus_state", height_le_bytes]`.
#[account]
#[derive(InitSpace)]
pub struct ConsensusStateStore {
    pub height: u64,
    pub consensus_state: ConsensusState,
}

impl ConsensusStateStore {
    pub const SEED: &'static [u8] = solana_ibc_types::ConsensusState::SEED;

    /// Returns `true` when `init_if_needed` just created the account
    /// (Tendermint heights start at 1, so zero means uninitialized).
    pub const fn is_uninitialized(&self) -> bool {
        self.height == 0
    }
}

/// Temporary storage for a single chunk of a Tendermint header during
/// multi-transaction upload.
///
/// Tendermint headers can exceed the Solana transaction size limit, so
/// they are uploaded in chunks across multiple transactions. Once all
/// chunks are present the `update_client` instruction reassembles and
/// verifies the full header, then closes these accounts to reclaim rent.
#[account]
#[derive(InitSpace)]
pub struct HeaderChunk {
    /// The submitter who created this chunk
    pub submitter: Pubkey,
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}

impl HeaderChunk {
    pub const SEED: &'static [u8] = b"header_chunk";
}

/// Temporary storage for a single chunk of misbehaviour evidence during
/// multi-transaction upload.
///
/// Similar to [`HeaderChunk`] but used for misbehaviour reports that prove
/// a validator set signed conflicting blocks. Once fully assembled, the
/// `misbehaviour` instruction verifies the evidence and freezes the client.
#[account]
#[derive(InitSpace)]
pub struct MisbehaviourChunk {
    /// The chunk data
    #[max_len(CHUNK_DATA_SIZE)]
    pub chunk_data: Vec<u8>,
}

impl MisbehaviourChunk {
    pub const SEED: &'static [u8] = b"misbehaviour_chunk";
}

/// Stores the result of an Ed25519 signature verification.
///
/// Because Solana's Ed25519 precompile writes results to a separate account
/// rather than returning them inline, this account acts as a bridge:
/// the precompile writes the verification outcome here, and the
/// `update_client` instruction reads it to confirm header signatures.
///
/// IMPORTANT: Field order matters — the verifier reads `data[8]` for `is_valid`.
#[account]
#[derive(InitSpace)]
pub struct SignatureVerification {
    /// Whether the signature is valid
    pub is_valid: bool,
    /// The submitter who created this verification
    pub submitter: Pubkey,
    /// `sha256(pk || msg || sig)`, used by the verifier to match accounts.
    pub sig_hash: [u8; 32],
}

impl SignatureVerification {
    pub const SEED: &'static [u8] = b"sig_verify";

    /// Anchor discriminator as a fixed-size array (length pinned by the assert below).
    pub fn discriminator_array() -> [u8; 8] {
        Self::DISCRIMINATOR
            .try_into()
            .expect("Anchor discriminator is always 8 bytes")
    }
}

// Compile-time verification that discriminator length matches the shared constant
const _: () = assert!(
    SignatureVerification::DISCRIMINATOR.len() == solana_ibc_constants::ANCHOR_DISCRIMINATOR_LEN
);

// Offset constants must match the Anchor layout.
const _: () = assert!(
    solana_ibc_types::ics07::SIGNATURE_VERIFICATION_SIG_HASH_OFFSET
        == solana_ibc_constants::ANCHOR_DISCRIMINATOR_LEN + 1 + 32
);
const _: () = assert!(
    solana_ibc_types::ics07::SIGNATURE_VERIFICATION_MIN_SIZE
        == solana_ibc_constants::ANCHOR_DISCRIMINATOR_LEN + 1 + 32 + 32
);

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use anchor_lang::AccountSerialize;
    use solana_ibc_types::ics07::{
        SIGNATURE_VERIFICATION_IS_VALID_OFFSET, SIGNATURE_VERIFICATION_MIN_SIZE,
        SIGNATURE_VERIFICATION_SIG_HASH_OFFSET,
    };

    /// Ensures `ConsensusStateStore` SEED constant matches solana-ibc-types
    #[test]
    fn test_consensus_state_store_seed_compatibility() {
        assert_eq!(
            ConsensusStateStore::SEED,
            solana_ibc_types::ConsensusState::SEED
        );
    }

    /// Trips on a field reorder of `SignatureVerification`. The verifier
    /// reads `is_valid` and `sig_hash` at fixed offsets.
    #[test]
    fn test_signature_verification_layout_sentinel() {
        let sentinel_sig_hash = [0xABu8; 32];
        let value = SignatureVerification {
            is_valid: true,
            submitter: Pubkey::new_unique(),
            sig_hash: sentinel_sig_hash,
        };

        let mut serialized = Vec::new();
        value
            .try_serialize(&mut serialized)
            .expect("AccountSerialize is infallible for fixed-size struct");

        assert!(serialized.len() >= SIGNATURE_VERIFICATION_MIN_SIZE);
        assert_eq!(
            serialized[SIGNATURE_VERIFICATION_IS_VALID_OFFSET], 1,
            "is_valid offset shifted"
        );
        assert_eq!(
            &serialized[SIGNATURE_VERIFICATION_SIG_HASH_OFFSET
                ..SIGNATURE_VERIFICATION_SIG_HASH_OFFSET + 32],
            &sentinel_sig_hash,
            "sig_hash offset shifted; SolanaSignatureVerifier would read wrong bytes"
        );
    }
}
