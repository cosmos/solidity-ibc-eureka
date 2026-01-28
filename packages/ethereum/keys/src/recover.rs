use alloy_primitives::{Address, Signature, B256};

/// Compute SHA-256 of message and recover the address from a 65-byte signature.
pub fn recover_address(message: &[u8], signature_65: &[u8]) -> Result<Address, anyhow::Error> {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(message);
    let prehash = B256::from_slice(&digest);
    recover_address_from_prehash(&prehash, signature_65)
}

/// Recover an Ethereum address from a prehash and 65-byte signature.
fn recover_address_from_prehash(
    prehash: &B256,
    signature_65: &[u8],
) -> Result<Address, anyhow::Error> {
    let sig = Signature::try_from(signature_65)
        .map_err(|_| anyhow::anyhow!("invalid signature length/format"))?;
    sig.recover_address_from_prehash(prehash)
        .map_err(|_| anyhow::anyhow!("signature recovery failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use sha2::Digest;

    #[test]
    fn recover_matches_signer_address() {
        let signer = PrivateKeySigner::random();
        let msg = b"abc";
        let digest = sha2::Sha256::digest(msg);
        let prehash = B256::from_slice(&digest);
        let sig = signer.sign_hash_sync(&prehash).unwrap();
        let sig_vec = sig.as_bytes().to_vec();
        let addr = recover_address_from_prehash(&prehash, &sig_vec).unwrap();
        assert_eq!(addr, signer.address());
        let addr2 = recover_address(msg, &sig_vec).unwrap();
        assert_eq!(addr2, signer.address());
    }

    #[test]
    fn invalid_signature_fails() {
        let msg = b"abc";
        let result = recover_address(msg, &[0u8; 64]);
        assert!(result.is_err());
    }
}
