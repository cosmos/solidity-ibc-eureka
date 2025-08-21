//! Test utilities for Attestor light client

pub use fixtures::*;

#[allow(
    missing_docs,
    clippy::borrow_interior_mutable_const,
    clippy::declare_interior_mutable_const,
    clippy::missing_panics_doc
)]
mod fixtures {
    use attestor_packet_membership::Packets;
    use k256::ecdsa::{signature::Signer, Signature, SigningKey, VerifyingKey};
    use sha2::{Digest, Sha256};
    use std::cell::LazyCell;

    pub const PACKET_COMMITMENTS: [&[u8; 12]; 3] =
        [b"cosmos rules", b"so does rust", b"hear, hear!!"];

    pub const PACKET_COMMITMENTS_ENCODED: LazyCell<Packets> =
        LazyCell::new(|| Packets::new(PACKET_COMMITMENTS.iter().map(|p| p.to_vec()).collect()));

    pub const S_KEYS: LazyCell<[SigningKey; 5]> = LazyCell::new(|| {
        [
            SigningKey::from_bytes(&[0xcd; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x02; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x03; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x10; 32].into()).expect("32 bytes, within curve order"),
            SigningKey::from_bytes(&[0x1F; 32].into()).expect("32 bytes, within curve order"),
        ]
    });

    pub const KEYS: LazyCell<Vec<VerifyingKey>> = LazyCell::new(|| {
        [
            *S_KEYS[0].verifying_key(),
            *S_KEYS[1].verifying_key(),
            *S_KEYS[2].verifying_key(),
            *S_KEYS[3].verifying_key(),
            *S_KEYS[4].verifying_key(),
        ]
        .to_vec()
    });

    pub const SIGS: LazyCell<Vec<Signature>> = LazyCell::new(|| {
        let sigs = S_KEYS
            .iter()
            .map(|skey| {
                let mut hasher = Sha256::new();
                let bytes: Vec<u8> = serde_json::to_vec(&(*PACKET_COMMITMENTS_ENCODED)).unwrap();
                hasher.update(&bytes);
                let hash_result = hasher.finalize();
                skey.sign(&hash_result)
            })
            .collect();

        sigs
    });

    #[must_use]
    pub fn packet_encoded_bytes() -> Vec<u8> {
        serde_json::to_vec(&(*PACKET_COMMITMENTS_ENCODED)).unwrap()
    }
}
