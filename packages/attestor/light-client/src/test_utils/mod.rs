//! Test utilities for Attestor light client

#[cfg(any(test, feature = "test-utils"))]
pub use fixtures::*;

#[allow(
    missing_docs,
    clippy::borrow_interior_mutable_const,
    clippy::declare_interior_mutable_const,
    clippy::missing_panics_doc
)]
#[cfg(any(test, feature = "test-utils"))]
mod fixtures {
    use alloy_primitives::{Address, FixedBytes, B256};
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use attestor_packet_membership::Packets;
    use sha2::{Digest, Sha256};
    use std::cell::LazyCell;

    pub const PACKET_COMMITMENTS: [[u8; 32]; 3] =
        [[1u8; 32], [2u8; 32], [3u8; 32]];

    pub const PACKET_COMMITMENTS_ENCODED: LazyCell<Packets> =
        LazyCell::new(|| Packets::new(PACKET_COMMITMENTS.iter().map(|p| FixedBytes::<32>::from(*p)).collect()));

    pub const S_SIGNERS: LazyCell<Vec<PrivateKeySigner>> = LazyCell::new(|| {
        vec![
            PrivateKeySigner::from_slice(&[0xcd; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x02; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x03; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x10; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x1F; 32]).expect("valid key"),
        ]
    });

    pub const KEYS: LazyCell<Vec<Address>> = LazyCell::new(|| {
        S_SIGNERS.iter().map(|s| s.address()).collect()
    });

    pub const ADDRESSES: LazyCell<Vec<Address>> = LazyCell::new(|| {
        // Keep a separate constant for tests that import ADDRESSES directly
        KEYS.clone()
    });

    pub const SIGS: LazyCell<Vec<Vec<u8>>> = LazyCell::new(|| {
        S_SIGNERS
            .iter()
            .map(|signer| {
                let mut hasher = Sha256::new();
                let bytes = PACKET_COMMITMENTS_ENCODED.to_abi_bytes();
                hasher.update(&bytes);
                let hash_result = hasher.finalize();
                let b256 = B256::from_slice(&hash_result);
                signer.sign_hash_sync(&b256).expect("signing should work").as_bytes().to_vec()
            })
            .collect()
    });
    
    pub const SIGS_RAW: LazyCell<Vec<Vec<u8>>> = LazyCell::new(|| {
        S_SIGNERS
            .iter()
            .map(|signer| {
                let mut hasher = Sha256::new();
                let bytes = PACKET_COMMITMENTS_ENCODED.to_abi_bytes();
                hasher.update(&bytes);
                let hash_result = hasher.finalize();
                let b256 = B256::from_slice(&hash_result);
                signer
                    .sign_hash_sync(&b256)
                    .expect("signing should work")
                    .as_bytes()
                    .to_vec()
            })
            .collect()
    });
}
