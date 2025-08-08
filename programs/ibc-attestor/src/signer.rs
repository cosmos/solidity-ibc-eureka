use key_utils::read_private_pem_to_secret;
use secp256k1::hashes::{sha256, Hash};
use secp256k1::Message;
use secp256k1::{SecretKey, SECP256K1};

use crate::cli::SignerConfig;
use crate::AttestorError;
use crate::{adapter_client::Signable, api::Attestation};

/// Signs `serde` encoded byte data using
/// the `secp256k1` algorithm.
pub struct Signer {
    skey: SecretKey,
}

impl Signer {
    pub fn from_config(config: SignerConfig) -> Result<Self, AttestorError> {
        let skey = read_private_pem_to_secret(config.secret_key)
            .map_err(|e| AttestorError::SignerConfigError(e.to_string()))?;
        Ok(Self { skey })
    }

    pub fn sign(&self, signable_data: impl Signable) -> Result<Attestation, AttestorError> {
        let bytes = signable_data.to_serde_encoded_bytes()?;
        let height = signable_data.height();
        let timestamp = signable_data.timestamp();

        let digest = sha256::Hash::hash(&bytes);
        let message = Message::from_digest(digest.to_byte_array());
        let sig = self.skey.sign_ecdsa(message);

        Ok(Attestation {
            height,
            timestamp,
            attested_data: bytes,
            public_key: self.get_pubkey(),
            signature: sig.serialize_compact().to_vec(),
        })
    }

    pub fn get_pubkey(&self) -> Vec<u8> {
        self.skey.public_key(SECP256K1).serialize().to_vec()
    }
}
