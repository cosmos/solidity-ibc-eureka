use std::fs;

use secp256k1::hashes::{sha256, Hash};
use secp256k1::Message;
use secp256k1::SecretKey;

use crate::cli::SignerConfig;
use crate::{adapter_client::Signable, attestation::Attestation};

pub struct Signer {
    skey: SecretKey,
}

impl Signer {
    pub fn from_config(config: SignerConfig) -> Self {
        let bytes = fs::read(&config.secret_key).unwrap();
        let skey = SecretKey::from_byte_array(bytes.try_into().unwrap()).unwrap();
        Self { skey }
    }

    pub fn sign(&self, signable_data: impl Signable) -> Attestation {
        let bytes = signable_data.bytes();

        let digest = sha256::Hash::hash(&bytes);
        let message = Message::from_digest(digest.to_byte_array());
        let sig = self.skey.sign_ecdsa(message);

        Attestation {
            data: bytes,
            signature: sig.serialize_compact(),
        }
    }
}
