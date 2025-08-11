//! Test utilities for Solana light client

use secp256k1::{ecdsa::Signature, hashes::Hash, Message, PublicKey, SecretKey};
use std::cell::LazyCell;

#[allow(missing_docs)]
pub const PACKET_COMMITMENTS: [&[u8; 12]; 3] = [b"cosmos rules", b"so does rust", b"hear, hear!!"];
#[allow(missing_docs)]
pub const PACKET_COMMITMENTS_ENCODED: LazyCell<Vec<u8>> =
    LazyCell::new(|| serde_json::to_vec(&PACKET_COMMITMENTS).unwrap());
#[allow(missing_docs)]
pub const S_KEYS: LazyCell<[SecretKey; 5]> = LazyCell::new(|| {
    [
        SecretKey::from_byte_array([0xcd; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_byte_array([0x02; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_byte_array([0x03; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_byte_array([0x10; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_byte_array([0x1F; 32]).expect("32 bytes, within curve order"),
    ]
});
#[allow(missing_docs)]
pub const KEYS: LazyCell<Vec<PublicKey>> = LazyCell::new(|| {
    [
        PublicKey::from_secret_key_global(&S_KEYS[0]),
        PublicKey::from_secret_key_global(&S_KEYS[1]),
        PublicKey::from_secret_key_global(&S_KEYS[2]),
        PublicKey::from_secret_key_global(&S_KEYS[3]),
        PublicKey::from_secret_key_global(&S_KEYS[4]),
    ]
    .to_vec()
});
#[allow(missing_docs)]
pub const SIGS: LazyCell<Vec<Signature>> = LazyCell::new(|| {
    let sigs = S_KEYS
        .iter()
        .map(|skey| {
            let digest = secp256k1::hashes::sha256::Hash::hash(&PACKET_COMMITMENTS_ENCODED);
            let message = Message::from_digest(digest.to_byte_array());
            skey.sign_ecdsa(message)
        })
        .collect();

    sigs
});

/// Returns all test public keys in compressed SEC1 format as a single contiguous byte blob.
/// Keys are concatenated in the same order as in KEYS, each 33 bytes long.
pub fn compressed_pubkeys_blob() -> Vec<u8> {
    let mut buf = Vec::with_capacity(33 * KEYS.len());
    for k in KEYS.iter() {
        buf.extend_from_slice(&k.serialize());
    }
    buf
}

/// Returns all test public keys in compressed SEC1 format as a vector of 33-byte arrays
pub fn compressed_pubkeys_vec() -> Vec<[u8; 33]> {
    KEYS.iter().map(|k| k.serialize()).collect()
}
