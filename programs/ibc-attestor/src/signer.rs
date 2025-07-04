use key_utils::read_private_pem_to_secret;
use secp256k1::hashes::{sha256, Hash};
use secp256k1::Message;
use secp256k1::SecretKey;
use thiserror::Error;

use crate::cli::SignerConfig;
use crate::{adapter_client::Signable, attestation_store::Attestation};

/// Signs `borsh` encoded byte data using
/// the `secp256k1` algorithm.
pub struct Signer {
    skey: SecretKey,
}

impl Signer {
    pub fn from_config(config: SignerConfig) -> Result<Self, SignerError> {
        let skey = read_private_pem_to_secret(config.secret_key)
            .map_err(|e| SignerError::SecretKeyError(e.to_string()))?;
        Ok(Self { skey })
    }

    pub fn sign(&self, signable_data: impl Signable) -> Attestation {
        let bytes = signable_data.to_encoded_bytes();

        let digest = sha256::Hash::hash(&bytes);
        let message = Message::from_digest(digest.to_byte_array());
        let sig = self.skey.sign_ecdsa(message);

        Attestation {
            data: bytes,
            signature: sig.serialize_compact(),
        }
    }
}

#[derive(Debug, Error)]
pub enum SignerError {
    #[error("failed to read secret due to {0}")]
    SecretKeyError(String),
}
