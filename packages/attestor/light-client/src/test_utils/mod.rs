//! Test utilities for Attestor light client

use attestor_packet_membership::Packets;
use alloy_primitives::keccak256;
use secp256k1::{ecdsa::RecoverableSignature, Message, Secp256k1, SecretKey};
use sha2::{Digest, Sha256};
use std::cell::LazyCell;

#[allow(missing_docs)]
pub const PACKET_COMMITMENTS: [&[u8; 12]; 3] = [b"cosmos rules", b"so does rust", b"hear, hear!!"];
#[allow(missing_docs)]
pub const PACKET_COMMITMENTS_ENCODED: LazyCell<Packets> =
    LazyCell::new(|| Packets::new(PACKET_COMMITMENTS.iter().map(|p| p.to_vec()).collect()));
#[allow(missing_docs)]
pub const S_KEYS: LazyCell<[SecretKey; 5]> = LazyCell::new(|| {
    [
        SecretKey::from_slice(&[0xcd; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_slice(&[0x02; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_slice(&[0x03; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_slice(&[0x10; 32]).expect("32 bytes, within curve order"),
        SecretKey::from_slice(&[0x1F; 32]).expect("32 bytes, within curve order"),
    ]
});
#[allow(missing_docs)]
pub const SIGNERS: LazyCell<Vec<[u8; 20]>> = LazyCell::new(|| {
    let secp = Secp256k1::new();
    S_KEYS
        .iter()
        .map(|sk| {
            let pk = secp256k1::PublicKey::from_secret_key(&secp, sk);
            let uncompressed = pk.serialize_uncompressed();
            let hash = keccak256(&uncompressed[1..]);
            let mut addr = [0u8; 20];
            addr.copy_from_slice(&hash[12..]);
            addr
        })
        .collect()
});
#[allow(missing_docs)]
pub const PUBKEYS: LazyCell<Vec<Vec<u8>>> = LazyCell::new(|| {
    let secp = Secp256k1::new();
    S_KEYS
        .iter()
        .map(|sk| {
            let pk = secp256k1::PublicKey::from_secret_key(&secp, sk);
            pk.serialize().to_vec() // compressed 33 bytes
        })
        .collect()
});
#[allow(missing_docs)]
pub const SIGS: LazyCell<Vec<Vec<u8>>> = LazyCell::new(|| {
    let secp = Secp256k1::new();
    S_KEYS
        .iter()
        .map(|sk| {
            let mut hasher = Sha256::new();
            let bytes = crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED);
            hasher.update(bytes);
            let digest = hasher.finalize();
            let msg = Message::from_digest_slice(&digest).expect("digest slice");
            let sig: RecoverableSignature = secp.sign_ecdsa_recoverable(&msg, sk);
            let (_rec_id, compact) = sig.serialize_compact();
            compact.to_vec() // 64-byte r||s
        })
        .collect()
});

#[allow(missing_docs)]
pub fn packet_encoded_bytes() -> Vec<u8> {
    PACKET_COMMITMENTS_ENCODED
        .packets()
        .flatten()
        .map(|p| p.clone())
        .collect()
}
