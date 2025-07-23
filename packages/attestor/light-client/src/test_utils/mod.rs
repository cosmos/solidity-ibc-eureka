//! Test utilities for Solana light client

use secp256k1::{ecdsa::Signature, hashes::Hash, Message, PublicKey, SecretKey};
use std::cell::LazyCell;

#[allow(missing_docs)]
pub const DUMMY_DATA: [u8; 1] = [0];
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
            let digest = secp256k1::hashes::sha256::Hash::hash(&DUMMY_DATA);
            let message = Message::from_digest(digest.to_byte_array());
            skey.sign_ecdsa(message)
        })
        .collect();

    sigs
});
