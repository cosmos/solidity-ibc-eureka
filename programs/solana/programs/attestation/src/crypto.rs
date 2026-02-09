use anchor_lang::prelude::*;

use crate::error::ErrorCode;

use solana_secp256k1_recover::{SECP256K1_PUBLIC_KEY_LENGTH, SECP256K1_SIGNATURE_LENGTH};

const SIGNATURE_LEN: usize = SECP256K1_SIGNATURE_LENGTH + 1;
const ETH_RECOVERY_ID_OFFSET: u8 = 27;
pub const ETH_ADDRESS_LEN: usize = 20;

pub type MessageHash = [u8; 32];

pub fn sha256_digest(data: &[u8]) -> MessageHash {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).into()
}

struct PreparedSignature {
    sig_bytes: [u8; SECP256K1_SIGNATURE_LENGTH],
    recovery_id: u8,
}

fn prepare_signature(signature: &[u8]) -> Result<PreparedSignature> {
    if signature.len() != SIGNATURE_LEN {
        return Err(error!(ErrorCode::InvalidSignature));
    }

    let mut sig_bytes = [0u8; SECP256K1_SIGNATURE_LENGTH];
    sig_bytes.copy_from_slice(&signature[..SECP256K1_SIGNATURE_LENGTH]);

    let v = signature[SECP256K1_SIGNATURE_LENGTH];

    Ok(PreparedSignature {
        sig_bytes,
        recovery_id: if v >= ETH_RECOVERY_ID_OFFSET {
            v - ETH_RECOVERY_ID_OFFSET
        } else {
            v
        },
    })
}

/// Recover Ethereum address from a precomputed message hash and a 65-byte secp256k1 signature.
#[cfg(target_os = "solana")]
pub fn recover_eth_address(
    message_hash: &MessageHash,
    signature: &[u8],
) -> Result<[u8; ETH_ADDRESS_LEN]> {
    let prepared = prepare_signature(signature)?;

    let pubkey = solana_secp256k1_recover::secp256k1_recover(
        message_hash,
        prepared.recovery_id,
        &prepared.sig_bytes,
    )
    .map_err(|_| error!(ErrorCode::InvalidSignature))?;

    Ok(pubkey_to_eth_address(&pubkey.0))
}

/// Recover Ethereum address from a precomputed message hash and a 65-byte secp256k1 signature (test impl).
#[cfg(all(not(target_os = "solana"), test))]
pub fn recover_eth_address(
    message_hash: &MessageHash,
    signature: &[u8],
) -> Result<[u8; ETH_ADDRESS_LEN]> {
    use alloy_primitives::Signature;

    let prepared = prepare_signature(signature)?;

    if prepared.recovery_id > 1 {
        return Err(error!(ErrorCode::InvalidSignature));
    }

    let sig = Signature::new(
        alloy_primitives::U256::from_be_slice(
            &prepared.sig_bytes[..SECP256K1_SIGNATURE_LENGTH / 2],
        ),
        alloy_primitives::U256::from_be_slice(
            &prepared.sig_bytes[SECP256K1_SIGNATURE_LENGTH / 2..],
        ),
        prepared.recovery_id != 0,
    );

    let address = sig
        .recover_address_from_prehash(&alloy_primitives::B256::from(*message_hash))
        .map_err(|_| error!(ErrorCode::InvalidSignature))?;

    Ok(address.0 .0)
}

#[cfg(all(not(target_os = "solana"), not(test)))]
pub fn recover_eth_address(
    _message_hash: &MessageHash,
    signature: &[u8],
) -> Result<[u8; ETH_ADDRESS_LEN]> {
    if signature.len() != SIGNATURE_LEN {
        return Err(error!(ErrorCode::InvalidSignature));
    }
    Err(error!(ErrorCode::InvalidSignature))
}

fn pubkey_to_eth_address(pubkey: &[u8; SECP256K1_PUBLIC_KEY_LENGTH]) -> [u8; ETH_ADDRESS_LEN] {
    let solana_keccak_hasher::Hash(hash) = solana_keccak_hasher::hash(pubkey);
    hash[hash.len() - ETH_ADDRESS_LEN..].try_into().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::short(vec![0u8; 64])]
    #[case::long(vec![0u8; 66])]
    #[case::empty(vec![])]
    fn test_recover_eth_address_invalid_signature_length(#[case] sig: Vec<u8>) {
        let hash = sha256_digest(b"test message");
        assert!(recover_eth_address(&hash, &sig).is_err());
    }

    #[test]
    fn test_recover_eth_address_recovery_id_normalization() {
        use crate::test_helpers::signing::TestAttestor;

        let attestor = TestAttestor::new(1);
        let message = b"test message for recovery id normalization";
        let hash = sha256_digest(message);
        let sig = attestor.sign(message);

        // Original signature uses Ethereum-style v (27 or 28)
        let original_v = sig[64];
        assert!(original_v == 27 || original_v == 28);

        // Recover with original Ethereum-style v
        let addr_original = recover_eth_address(&hash, &sig).unwrap();

        // Recover with canonical v (0 or 1)
        let mut sig_canonical = sig;
        sig_canonical[64] = original_v - 27;
        let addr_canonical = recover_eth_address(&hash, &sig_canonical).unwrap();

        // Both should recover the same address
        assert_eq!(addr_original, addr_canonical);
        assert_eq!(addr_original, attestor.eth_address);
    }

    #[test]
    fn test_recover_eth_address_invalid_recovery_id() {
        use crate::test_helpers::signing::TestAttestor;

        let attestor = TestAttestor::new(1);
        let message = b"test message for invalid recovery id";
        let hash = sha256_digest(message);
        let mut sig = attestor.sign(message);

        // v=29 normalizes to 2, which is invalid for secp256k1
        sig[64] = 29;
        assert!(recover_eth_address(&hash, &sig).is_err());
    }
}
