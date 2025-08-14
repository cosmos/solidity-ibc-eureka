//! Test utilities for Attestor light client

use k256::ecdsa::{signature::Signer, Signature, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use std::cell::LazyCell;

#[allow(missing_docs)]
pub const PACKET_COMMITMENTS: [&[u8; 12]; 3] = [b"cosmos rules", b"so does rust", b"hear, hear!!"];
#[allow(missing_docs)]
pub const PACKET_COMMITMENTS_ENCODED: LazyCell<Vec<u8>> =
    LazyCell::new(|| serde_json::to_vec(&PACKET_COMMITMENTS).unwrap());
#[allow(missing_docs)]
pub const S_KEYS: LazyCell<[SigningKey; 5]> = LazyCell::new(|| {
    [
        SigningKey::from_bytes(&[0xcd; 32].into()).expect("32 bytes, within curve order"),
        SigningKey::from_bytes(&[0x02; 32].into()).expect("32 bytes, within curve order"),
        SigningKey::from_bytes(&[0x03; 32].into()).expect("32 bytes, within curve order"),
        SigningKey::from_bytes(&[0x10; 32].into()).expect("32 bytes, within curve order"),
        SigningKey::from_bytes(&[0x1F; 32].into()).expect("32 bytes, within curve order"),
    ]
});
#[allow(missing_docs)]
pub const KEYS: LazyCell<Vec<VerifyingKey>> = LazyCell::new(|| {
    [
        S_KEYS[0].verifying_key().clone(),
        S_KEYS[1].verifying_key().clone(),
        S_KEYS[2].verifying_key().clone(),
        S_KEYS[3].verifying_key().clone(),
        S_KEYS[4].verifying_key().clone(),
    ]
    .to_vec()
});
#[allow(missing_docs)]
pub const SIGS: LazyCell<Vec<Signature>> = LazyCell::new(|| {
    let sigs = S_KEYS
        .iter()
        .map(|skey| {
            let mut hasher = Sha256::new();
            hasher.update(&*PACKET_COMMITMENTS_ENCODED);
            let hash_result = hasher.finalize();
            skey.sign(&hash_result)
        })
        .collect();

    sigs
});
