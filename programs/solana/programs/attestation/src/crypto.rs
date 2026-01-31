//! Cryptographic utilities for Ethereum signature verification on Solana.
//!
//! This module provides secp256k1 signature recovery and Ethereum address
//! derivation with three implementations based on compilation target:
//!
//! ## Production (`target_os = "solana"`)
//! Uses Solana's `sol_secp256k1_recover` syscall - a precompiled, audited
//! implementation provided by the Solana runtime. This is the only version
//! that runs on-chain.
//!
//! ## Tests (`#[cfg(test)]`)
//! Uses the `k256` crate (pure Rust secp256k1) for native test execution.
//! This allows unit tests to run without the Solana runtime while maintaining
//! cryptographic correctness. The k256 crate is well-audited and produces
//! identical results to the Solana syscall.
//!
//! **Why k256 in tests?** Solana syscalls are only available in the SVM runtime.
//! Native `cargo test` runs outside SVM, so we need a compatible implementation.
//! Mollusk tests (which simulate SVM) use the real syscall via the compiled program.
//!
//! ## IDL Generation (`#[cfg(not(test))]` on native)
//! Returns a stub error since signature recovery isn't needed for IDL extraction.
//!
//! ## Security Considerations
//! - Production security relies entirely on Solana's syscall implementation
//! - Test implementation uses k256 which is widely used and audited
//! - Both implementations use identical message hashing (SHA256) and address
//!   derivation (keccak256 of uncompressed pubkey, take last 20 bytes)

use anchor_lang::prelude::*;

use crate::error::ErrorCode;

const SIGNATURE_LEN: usize = 65;
const ETH_ADDRESS_LEN: usize = 20;
const KECCAK256_HASH_LEN: usize = 32;
const SECP256K1_PUBLIC_KEY_LENGTH: usize = 64;

/// Prepared signature data for secp256k1 recovery.
struct PreparedSignature {
    message_hash: [u8; 32],
    sig_bytes: [u8; 64],
    recovery_id: u8,
}

/// Validate signature and prepare data for secp256k1 recovery.
///
/// Performs:
/// - Signature length validation (must be 65 bytes)
/// - SHA256 message hashing
/// - Recovery ID normalization (27/28 → 0/1)
fn prepare_signature(message: &[u8], signature: &[u8]) -> Result<PreparedSignature> {
    use sha2::{Digest, Sha256};

    if signature.len() != SIGNATURE_LEN {
        return Err(error!(ErrorCode::InvalidSignature));
    }

    let message_hash: [u8; 32] = Sha256::digest(message).into();

    let recovery_id = signature[64];
    let recovery_id = if recovery_id >= 27 {
        recovery_id.saturating_sub(27)
    } else {
        recovery_id
    };

    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&signature[..64]);

    Ok(PreparedSignature {
        message_hash,
        sig_bytes,
        recovery_id,
    })
}

#[cfg(target_os = "solana")]
solana_define_syscall::define_syscall!(fn sol_secp256k1_recover(hash: *const u8, recovery_id: u64, signature: *const u8, result: *mut u8) -> u64);

/// Recover Ethereum address from a signature using Solana's `secp256k1_recover` syscall.
///
/// Signature format: `r[32] || s[32] || v[1]` where v is the recovery ID (27/28 or 0/1).
#[cfg(target_os = "solana")]
pub fn recover_eth_address(message: &[u8], signature: &[u8]) -> Result<[u8; ETH_ADDRESS_LEN]> {
    let prepared = prepare_signature(message, signature)?;

    let mut pubkey = [0u8; SECP256K1_PUBLIC_KEY_LENGTH];

    // Unsafe is required because we're calling a Solana syscall via raw pointers.
    // The syscall is implemented in C and exposed as an external function, so Rust
    // cannot verify memory safety at compile time. We must manually ensure:
    // - message_hash points to valid 32-byte buffer (guaranteed by [u8; 32] type)
    // - sig_bytes points to valid 64-byte buffer (guaranteed by [u8; 64] type)
    // - pubkey points to valid 64-byte writable buffer (guaranteed by [u8; 64] type)
    // The syscall will write the recovered public key into pubkey buffer.
    let ret = unsafe {
        sol_secp256k1_recover(
            prepared.message_hash.as_ptr(),
            prepared.recovery_id as u64,
            prepared.sig_bytes.as_ptr(),
            pubkey.as_mut_ptr(),
        )
    };

    if ret != 0 {
        msg!("secp256k1_recover failed with code: {}", ret);
        return Err(error!(ErrorCode::InvalidSignature));
    }

    Ok(pubkey_to_eth_address(&pubkey))
}

/// Native test implementation using k256 for signature recovery.
///
/// k256 is a pure-Rust, constant-time implementation of secp256k1. It's used by
/// major projects (ethers-rs, alloy) and has been audited. The cryptographic
/// operations are identical to libsecp256k1 which Solana's syscall uses.
#[cfg(all(not(target_os = "solana"), test))]
pub fn recover_eth_address(message: &[u8], signature: &[u8]) -> Result<[u8; ETH_ADDRESS_LEN]> {
    use k256::ecdsa::{RecoveryId, Signature, VerifyingKey};

    let prepared = prepare_signature(message, signature)?;

    let sig = Signature::from_slice(&prepared.sig_bytes)
        .map_err(|_| error!(ErrorCode::InvalidSignature))?;

    let rec_id = RecoveryId::try_from(prepared.recovery_id)
        .map_err(|_| error!(ErrorCode::InvalidSignature))?;

    let verifying_key = VerifyingKey::recover_from_prehash(&prepared.message_hash, &sig, rec_id)
        .map_err(|_| error!(ErrorCode::InvalidSignature))?;

    let pubkey_bytes = verifying_key.to_encoded_point(false);
    let pubkey_uncompressed = &pubkey_bytes.as_bytes()[1..]; // Skip 0x04 prefix

    let mut pubkey = [0u8; SECP256K1_PUBLIC_KEY_LENGTH];
    pubkey.copy_from_slice(pubkey_uncompressed);

    Ok(pubkey_to_eth_address(&pubkey))
}

/// Stub for non-Solana, non-test builds (IDL generation)
#[cfg(all(not(target_os = "solana"), not(test)))]
pub fn recover_eth_address(_message: &[u8], signature: &[u8]) -> Result<[u8; ETH_ADDRESS_LEN]> {
    if signature.len() != SIGNATURE_LEN {
        return Err(error!(ErrorCode::InvalidSignature));
    }
    Err(error!(ErrorCode::InvalidSignature))
}

/// Convert a 64-byte secp256k1 public key to Ethereum address.
///
/// Ethereum address = last 20 bytes of `keccak256(uncompressed_pubkey)`
fn pubkey_to_eth_address(pubkey: &[u8; 64]) -> [u8; ETH_ADDRESS_LEN] {
    let hash = keccak256(pubkey);
    let mut address = [0u8; ETH_ADDRESS_LEN];
    address.copy_from_slice(&hash[12..32]);
    address
}

/// Compute keccak256 hash using Solana's native hasher.
pub fn keccak256(data: &[u8]) -> [u8; KECCAK256_HASH_LEN] {
    solana_keccak_hasher::hash(data).0
}

/// Compute keccak256 hash of a path (for IBC commitment paths).
pub fn hash_path(path: &[u8]) -> [u8; 32] {
    keccak256(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keccak256() {
        let data = b"hello";
        let hash = keccak256(data);
        assert_eq!(hash.len(), 32);
        let expected =
            hex::decode("1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8")
                .unwrap();
        assert_eq!(hash.as_slice(), expected.as_slice());
    }

    #[test]
    fn test_keccak256_empty() {
        let hash = keccak256(b"");
        let expected =
            hex::decode("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470")
                .unwrap();
        assert_eq!(hash.as_slice(), expected.as_slice());
    }

    #[test]
    fn test_pubkey_to_eth_address() {
        let pubkey = [0u8; 64];
        let address = pubkey_to_eth_address(&pubkey);
        assert_eq!(address.len(), 20);
    }

    #[test]
    fn test_pubkey_to_eth_address_known_value() {
        let mut pubkey = [0u8; 64];
        pubkey[0] = 0x04;
        let address = pubkey_to_eth_address(&pubkey);
        let expected_hash = keccak256(&pubkey);
        assert_eq!(address, expected_hash[12..32]);
    }

    #[test]
    fn test_hash_path() {
        let path = b"ibc/commitments/channel-0/sequence/1";
        let hash = hash_path(path);
        assert_eq!(hash.len(), 32);
        assert_eq!(hash, keccak256(path));
    }

    #[test]
    fn test_hash_path_empty() {
        let hash = hash_path(b"");
        assert_eq!(hash, keccak256(b""));
    }

    #[test]
    fn test_hash_path_deterministic() {
        let path = b"test/path/to/commitment";
        let hash1 = hash_path(path);
        let hash2 = hash_path(path);
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_hash_path_different_inputs() {
        let hash1 = hash_path(b"path1");
        let hash2 = hash_path(b"path2");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_recover_eth_address_invalid_signature_length_short() {
        let message = b"test message";
        let short_sig = vec![0u8; 64];
        assert!(recover_eth_address(message, &short_sig).is_err());
    }

    #[test]
    fn test_recover_eth_address_invalid_signature_length_long() {
        let message = b"test message";
        let long_sig = vec![0u8; 66];
        assert!(recover_eth_address(message, &long_sig).is_err());
    }

    #[test]
    fn test_recover_eth_address_empty_signature() {
        let message = b"test message";
        let empty_sig: Vec<u8> = vec![];
        assert!(recover_eth_address(message, &empty_sig).is_err());
    }
}
