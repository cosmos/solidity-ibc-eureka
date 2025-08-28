use alloy_primitives::Signature;
use alloy_signer_local::PrivateKeySigner;
use ethereum_keys::signature::sign;
use ethereum_keys::signer_local::read_from_keystore;

use crate::cli::SignerConfig;
use crate::AttestorError;
use crate::{adapter_client::Signable, api::Attestation};

/// Signs `serde` encoded byte data using
/// the `secp256k1` algorithm.
pub struct Signer {
    signer: PrivateKeySigner,
}

impl Signer {
    pub fn from_config(config: SignerConfig) -> Result<Self, AttestorError> {
        let signer = read_from_keystore(config.keystore_path)
            .map_err(|e| AttestorError::SignerConfigError(e.to_string()))?;
        Ok(Self { signer })
    }

    pub fn sign(&self, signable_data: impl Signable) -> Result<Attestation, AttestorError> {
        let bytes = signable_data
            .to_abi_encoded_bytes()
            .map_err(|e| AttestorError::SignerError(e.to_string()))?;
        let height = signable_data.height();
        let timestamp = signable_data.timestamp();

        let sig: Signature =
            sign(&self.signer, &bytes).map_err(|e| AttestorError::SignerError(e.to_string()))?;

        let sig65 = sig.as_bytes().to_vec();

        Ok(Attestation {
            height,
            timestamp,
            attested_data: bytes,
            signature: sig65,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use sha2::{Digest, Sha256};

    struct MockSignable {
        data: Vec<u8>,
        height: u64,
        timestamp: Option<u64>,
    }

    impl Signable for MockSignable {
        fn to_abi_encoded_bytes(&self) -> Result<Vec<u8>, alloy_sol_types::Error> {
            Ok(self.data.clone())
        }

        fn height(&self) -> u64 {
            self.height
        }

        fn timestamp(&self) -> Option<u64> {
            self.timestamp
        }
    }

    #[test]
    fn test_signature_is_now_65_bytes() {
        let signer = Signer {
            signer: PrivateKeySigner::random(),
        };

        let mock_data = MockSignable {
            data: b"test data".to_vec(),
            height: 100,
            timestamp: Some(1234567890),
        };

        let attestation = signer.sign(mock_data).unwrap();

        assert_eq!(
            attestation.signature.len(),
            65,
            "Updated implementation produces 65-byte signatures"
        );

        // Verify the v byte is in the correct range
        let v = attestation.signature[64];
        assert!(
            v == 27 || v == 28 || v == 0 || v == 1,
            "v should be 27/28 or 0/1 for Ethereum compatibility"
        );
    }

    #[test]
    fn test_65_byte_signature_with_recovery_id() {
        use alloy_primitives::{Signature as AlloySignature, B256};

        let signer = PrivateKeySigner::random();

        let data = b"test data";
        let digest = Sha256::digest(data);
        let hash = B256::from_slice(&digest);

        let sig: AlloySignature = signer.sign_hash_sync(&hash).unwrap();
        let sig65 = sig.as_bytes().to_vec();

        assert_eq!(
            sig65.len(),
            65,
            "65-byte signature should include r, s, and v"
        );

        // Test that the signature can be parsed by alloy and used for recovery
        let alloy_sig = AlloySignature::try_from(sig65.as_slice()).unwrap();
        let recovered_address = alloy_sig.recover_address_from_prehash(&hash).unwrap();

        // Expected address is the signer's address
        let expected_address = signer.address();

        assert_eq!(
            recovered_address, expected_address,
            "recovered address should match signer's address"
        );

        // Also verify the recovery id is in valid range (27/28 or 0/1)
        let v = sig65[64];
        assert!(
            v == 27 || v == 28 || v == 0 || v == 1,
            "v should be 27/28 or 0/1 for Ethereum compatibility"
        );
    }
}
