//! Test utilities for Attestor light client

#[cfg(any(test, feature = "test-utils"))]
pub use fixtures::*;

#[allow(
    missing_docs,
    clippy::borrow_interior_mutable_const,
    clippy::declare_interior_mutable_const,
    clippy::missing_panics_doc,
    clippy::redundant_closure_for_method_calls
)]
#[cfg(any(test, feature = "test-utils"))]
mod fixtures {
    use alloy_primitives::{Address, B256};
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use alloy_sol_types::SolValue;
    use ibc_eureka_solidity_types::msgs::IAttestorMsgs::{PacketAttestation, PacketCompact};
    use sha2::{Digest, Sha256};
    use std::cell::LazyCell;

    #[must_use]
    pub fn sample_packet_commitments() -> Vec<PacketCompact> {
        vec![
            PacketCompact {
                path: [0x11u8; 32].into(),
                commitment: [0x12u8; 32].into(),
            },
            PacketCompact {
                path: [0x21u8; 32].into(),
                commitment: [0x22u8; 32].into(),
            },
            PacketCompact {
                path: [0x31u8; 32].into(),
                commitment: [0x32u8; 32].into(),
            },
        ]
    }

    pub const PACKET_COMMITMENTS_ENCODED: LazyCell<PacketAttestation> = LazyCell::new(|| {
        PacketAttestation {
            // TODO: Needs to be real value
            height: 0,
            packets: sample_packet_commitments(),
        }
    });

    pub const S_SIGNERS: LazyCell<Vec<PrivateKeySigner>> = LazyCell::new(|| {
        vec![
            PrivateKeySigner::from_slice(&[0xcd; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x02; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x03; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x10; 32]).expect("valid key"),
            PrivateKeySigner::from_slice(&[0x1F; 32]).expect("valid key"),
        ]
    });

    pub const KEYS: LazyCell<Vec<Address>> =
        LazyCell::new(|| S_SIGNERS.iter().map(|s| s.address()).collect());

    pub const ADDRESSES: LazyCell<Vec<Address>> = LazyCell::new(|| {
        // Keep a separate constant for tests that import ADDRESSES directly
        KEYS.clone()
    });

    pub const SIGS_RAW: LazyCell<Vec<Vec<u8>>> = LazyCell::new(|| {
        S_SIGNERS
            .iter()
            .map(|signer| {
                let mut hasher = Sha256::new();
                let bytes = PACKET_COMMITMENTS_ENCODED.abi_encode();
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

    #[must_use]
    pub fn packet_encoded_bytes() -> Vec<u8> {
        serde_json::to_vec(&(*PACKET_COMMITMENTS_ENCODED)).unwrap()
    }

    #[must_use]
    #[allow(clippy::borrow_interior_mutable_const)]
    pub fn keys() -> Vec<VerifyingKey> {
        KEYS.clone()
    }

    #[must_use]
    #[allow(clippy::borrow_interior_mutable_const)]
    pub fn sigs() -> Vec<Signature> {
        SIGS.clone()
    }

    #[must_use]
    #[allow(clippy::borrow_interior_mutable_const)]
    pub fn packets() -> Packets {
        (*PACKET_COMMITMENTS_ENCODED).clone()
    }
}
