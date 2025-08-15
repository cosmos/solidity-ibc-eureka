use k256::ecdsa::SigningKey;
use key_utils::read_private_pem_to_secret;
use sha2::{Digest, Sha256};

use crate::cli::SignerConfig;
use crate::AttestorError;
use crate::{adapter_client::Signable, api::Attestation};

/// Signs `serde` encoded byte data using
/// the `secp256k1` algorithm.
pub struct Signer {
    signing_key: SigningKey,
}

impl Signer {
    pub fn from_config(config: SignerConfig) -> Result<Self, AttestorError> {
        let secret_key = read_private_pem_to_secret(config.secret_key)
            .map_err(|e| AttestorError::SignerConfigError(e.to_string()))?;
        let signing_key = SigningKey::from(secret_key);
        Ok(Self { signing_key })
    }

    pub fn sign(&self, signable_data: impl Signable) -> Result<Attestation, AttestorError> {
        let bytes = signable_data.to_serde_encoded_bytes()?;
        let height = signable_data.height();
        let timestamp = signable_data.timestamp();

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let digest = hasher.finalize();

        let (sig, _) = self
            .signing_key
            .sign_recoverable(&digest)
            .map_err(|e| AttestorError::SignerError(e.to_string()))?;

        Ok(Attestation {
            height,
            timestamp,
            attested_data: bytes,
            public_key: self.get_pubkey(),
            signature: sig.to_bytes().to_vec(),
        })
    }

    pub fn get_pubkey(&self) -> Vec<u8> {
        self.signing_key
            .verifying_key()
            .to_encoded_point(true)
            .as_bytes()
            .to_vec()
    }
}
