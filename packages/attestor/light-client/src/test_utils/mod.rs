//! Test utilities for Attestor light client

pub use fixtures::*;

#[allow(
    missing_docs,
    clippy::borrow_interior_mutable_const,
    clippy::declare_interior_mutable_const,
    clippy::missing_panics_doc
)]
mod fixtures {
    use alloy_primitives::{Address, FixedBytes};
    use attestor_packet_membership::Packets;
    use k256::ecdsa::{signature::Signer, Signature, SigningKey};
    use sha2::{Digest, Sha256};
    use std::cell::LazyCell;

    pub const PACKET_COMMITMENTS: [[u8; 32]; 3] =
        [[1u8; 32], [2u8; 32], [3u8; 32]];

    pub const PACKET_COMMITMENTS_ENCODED: LazyCell<Packets> =
        LazyCell::new(|| Packets::new(PACKET_COMMITMENTS.iter().map(|p| FixedBytes::<32>::from(*p)).collect()));

    pub const S_KEYS: LazyCell<[SigningKey; 5]> = LazyCell::new(|| {
        [
            SigningKey::from_bytes(&[0xcd; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x02; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x03; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x10; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x1F; 32].into()).expect("32 bytes, within curve order"),
        ]
    });

    pub const KEYS: LazyCell<Vec<Address>> = LazyCell::new(|| {
        use sha3::{Digest as Sha3Digest, Keccak256};
        // Derive Ethereum addresses from the verifying keys corresponding to our signing keys
        S_KEYS
            .iter()
            .map(|skey| {
                let pubkey_bytes = skey.verifying_key().to_encoded_point(false);
                // Keccak256 over uncompressed pubkey without the 0x04 prefix, take last 20 bytes
                let hash = Keccak256::digest(&pubkey_bytes.as_bytes()[1..]);
                let mut addr_bytes = [0u8; 20];
                addr_bytes.copy_from_slice(&hash[12..]);
                Address::from(addr_bytes)
            })
            .collect()
    });

    pub const ADDRESSES: LazyCell<Vec<Address>> = LazyCell::new(|| {
        // Keep a separate constant for tests that import ADDRESSES directly
        KEYS.clone()
    });

    pub const SIGS: LazyCell<Vec<Signature>> = LazyCell::new(|| {
        let sigs = S_KEYS
            .iter()
            .map(|skey| {
                let mut hasher = Sha256::new();
                let bytes = PACKET_COMMITMENTS_ENCODED.to_abi_bytes();
                hasher.update(&bytes);
                let hash_result = hasher.finalize();
                skey.sign(&hash_result)
            })
            .collect();

        sigs
    });
    
    pub const SIGS_RAW: LazyCell<Vec<Vec<u8>>> = LazyCell::new(|| {
        use k256::ecdsa::{RecoveryId};
        
        S_KEYS.iter().map(|skey| {
            let mut hasher = Sha256::new();
            let bytes = PACKET_COMMITMENTS_ENCODED.to_abi_bytes();
            hasher.update(&bytes);
            let hash_result = hasher.finalize();
            
            // Sign the message with recovery
            let (sig, recovery_id): (k256::ecdsa::Signature, RecoveryId) = skey
                .sign_prehash_recoverable(&hash_result)
                .expect("signing should work");
            
            let (r, s) = sig.split_bytes();
            
            let mut sig_bytes = Vec::with_capacity(65);
            sig_bytes.extend_from_slice(&r);
            sig_bytes.extend_from_slice(&s);
            // Use the recovery ID directly from k256
            sig_bytes.push(recovery_id.to_byte());
            sig_bytes
        }).collect()
    });

    #[must_use]
    pub fn packet_encoded_bytes() -> Vec<u8> {
        PACKET_COMMITMENTS_ENCODED.to_abi_bytes()
    }
}
