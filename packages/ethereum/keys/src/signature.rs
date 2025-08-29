use alloy_primitives::{Signature, B256};

#[cfg(feature = "signer")]
use alloy_signer::SignerSync;

use sha2::{Digest, Sha256};

/// Compute SHA-256 of input and sign the digest.
#[cfg(feature = "signer")]
pub fn sign<T: SignerSync>(signer: &T, message: &[u8]) -> Result<Signature, anyhow::Error> {
    let digest = Sha256::digest(message);
    let hash = B256::from_slice(&digest);

    signer
        .sign_hash_sync(&hash)
        .map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recover::recover_address;
    use alloy_primitives::Address;
    use alloy_signer_local::PrivateKeySigner;
    use sha2::Digest;

    /// Verify a single signature against an expected address, for a raw message (SHA-256 prehashing).
    fn verify_signature(expected: Address, message: &[u8], signature_65: &[u8]) -> bool {
        recover_address(message, signature_65)
            .map(|addr| addr == expected)
            .unwrap_or(false)
    }

    #[test]
    fn sign_sha256_produces_65_bytes_and_recoverable() {
        let signer = PrivateKeySigner::random();
        let message = b"hello";

        let sig = sign(&signer, message).unwrap();
        let bytes = sig.as_bytes();
        assert_eq!(bytes.len(), 65);

        // recovery id
        let v = bytes[64];
        assert!(v == 27 || v == 28 || v == 0 || v == 1);

        // recovery
        let digest = alloy_primitives::B256::from_slice(&Sha256::digest(message));
        let addr = sig.recover_address_from_prehash(&digest).unwrap();
        assert_eq!(addr, signer.address());
    }

    #[test]
    fn verify_true_and_false_cases() {
        let signer = PrivateKeySigner::random();
        let addr = signer.address();
        let msg = b"abc";

        let sig: Signature = sign(&signer, msg).unwrap();
        let sig_vec = sig.as_bytes().to_vec();
        assert!(verify_signature(addr, msg, &sig_vec));

        // verify wrong address fails
        let wrong = Address::from([0x11; 20]);
        assert!(!verify_signature(wrong, msg, &sig_vec));

        // verify tamper message fails
        assert!(!verify_signature(addr, b"abcd", &sig_vec));

        // verify corrupt signature fails
        let mut bad = sig_vec.clone();
        bad[10] ^= 0xFF;
        assert!(!verify_signature(addr, msg, &bad));
    }
}
