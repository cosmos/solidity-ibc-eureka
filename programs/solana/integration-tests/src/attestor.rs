//! ECDSA attestor for signing attestation proofs in integration tests.
//!
//! Replicates the `TestAttestor` from `attestation::test_helpers::signing`
//! (which is `#[cfg(test)]`-gated and inaccessible from this crate).
//!
//! Attestors are off-chain signers (Ethereum ECDSA keys) that do not hold
//! a Solana keypair, so they do not implement the [`Actor`](crate::Actor)
//! trait and live outside the `actors/` directory.

use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use attestation::crypto::AttestationType;
use sha2::{Digest, Sha256};

const ETH_ADDRESS_LEN: usize = 20;
const DOMAIN_SEPARATED_PREIMAGE_LEN: usize = 1 + 32;

/// Off-chain ECDSA signer that produces attestation signatures consumed by
/// the relayer. Has no Solana keypair and does not submit transactions.
pub struct Attestor {
    signer: PrivateKeySigner,
    pub eth_address: [u8; ETH_ADDRESS_LEN],
}

impl Attestor {
    /// Create an attestor from a deterministic seed byte.
    pub fn new(seed: u8) -> Self {
        let mut key_bytes = [0u8; 32];
        key_bytes[0] = seed;
        key_bytes[31] = 1; // ensure non-zero

        let signer =
            PrivateKeySigner::from_bytes(&key_bytes.into()).expect("valid key bytes for testing");
        let eth_address = signer.address().0 .0;

        Self {
            signer,
            eth_address,
        }
    }

    /// Sign attestation data with domain separation and return a 65-byte signature.
    ///
    /// Computes `sha256(type_tag || sha256(data))` then signs the result.
    pub fn sign(&self, data: &[u8], attestation_type: AttestationType) -> Vec<u8> {
        let inner_hash: [u8; 32] = Sha256::digest(data).into();
        let mut tagged = Vec::with_capacity(DOMAIN_SEPARATED_PREIMAGE_LEN);
        tagged.push(attestation_type as u8);
        tagged.extend_from_slice(&inner_hash);
        let message_hash: [u8; 32] = Sha256::digest(&tagged).into();

        let sig = self
            .signer
            .sign_hash_sync(&message_hash.into())
            .expect("signing should succeed");

        let mut result = Vec::with_capacity(65);
        result.extend_from_slice(&sig.r().to_be_bytes::<32>());
        result.extend_from_slice(&sig.s().to_be_bytes::<32>());
        result.push(u8::from(sig.v()) + 27);
        result
    }
}

/// A set of attestors with deterministic keys.
pub struct Attestors(Vec<Attestor>);

impl Attestors {
    /// Create `count` attestors with deterministic keys (seeds 1..=count).
    pub fn new(count: usize) -> Self {
        Self((1..=count as u8).map(Attestor::new).collect())
    }

    pub fn as_slice(&self) -> &[Attestor] {
        &self.0
    }

    pub fn eth_addresses(&self) -> Vec<[u8; ETH_ADDRESS_LEN]> {
        self.0.iter().map(|a| a.eth_address).collect()
    }
}
