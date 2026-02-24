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
    use ibc_eureka_solidity_types::msgs::IAttestationMsgs::{PacketAttestation, PacketCompact};
    use sha2::{Digest, Sha256};
    use std::cell::LazyCell;

    const STATE_TAG: u8 = 0x01;
    const PACKET_TAG: u8 = 0x02;

    /// Length of the domain-separated signing preimage: 1-byte type tag + 32-byte SHA-256 hash.
    const DOMAIN_SEPARATED_PREIMAGE_LEN: usize = 1 + 32;

    fn tagged_signing_input(data: &[u8], tag: u8) -> B256 {
        let inner_hash = Sha256::digest(data);
        let mut tagged = Vec::with_capacity(DOMAIN_SEPARATED_PREIMAGE_LEN);
        tagged.push(tag);
        tagged.extend_from_slice(&inner_hash);
        B256::from_slice(&Sha256::digest(&tagged))
    }

    pub const MEMBERSHIP_PATH: &[u8] = b"membership-path";
    pub const NON_MEMBERSHIP_PATH: &[u8] = b"non-membership-path";

    #[must_use]
    pub fn sample_packet_commitments() -> Vec<PacketCompact> {
        vec![
            PacketCompact {
                path: alloy_primitives::keccak256(MEMBERSHIP_PATH),
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
            // Non-membership packet with zero commitment for timeout testing
            PacketCompact {
                path: alloy_primitives::keccak256(NON_MEMBERSHIP_PATH),
                commitment: [0u8; 32].into(),
            },
        ]
    }

    #[must_use]
    pub fn packet_commitments_with_height(height: u64) -> PacketAttestation {
        let mut packets = PACKET_COMMITMENTS_ENCODED.clone();
        packets.height = height;
        packets
    }

    #[must_use]
    pub fn sigs_with_height(height: u64) -> Vec<Vec<u8>> {
        S_SIGNERS
            .iter()
            .map(|signer| {
                let bytes = packet_commitments_with_height(height).abi_encode();
                let b256 = tagged_signing_input(&bytes, PACKET_TAG);
                signer
                    .sign_hash_sync(&b256)
                    .expect("signing should work")
                    .as_bytes()
                    .to_vec()
            })
            .collect()
    }

    pub const PACKET_COMMITMENTS_ENCODED: LazyCell<PacketAttestation> =
        LazyCell::new(|| PacketAttestation {
            height: 0,
            packets: sample_packet_commitments(),
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
                let bytes = PACKET_COMMITMENTS_ENCODED.abi_encode();
                let b256 = tagged_signing_input(&bytes, PACKET_TAG);
                signer
                    .sign_hash_sync(&b256)
                    .expect("signing should work")
                    .as_bytes()
                    .to_vec()
            })
            .collect()
    });

    pub const STATE_SIGS_RAW: LazyCell<Vec<Vec<u8>>> = LazyCell::new(|| {
        S_SIGNERS
            .iter()
            .map(|signer| {
                let bytes = PACKET_COMMITMENTS_ENCODED.abi_encode();
                let b256 = tagged_signing_input(&bytes, STATE_TAG);
                signer
                    .sign_hash_sync(&b256)
                    .expect("signing should work")
                    .as_bytes()
                    .to_vec()
            })
            .collect()
    });
}
