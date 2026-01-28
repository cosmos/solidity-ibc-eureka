use crate::error::ErrorCode;
use crate::types::{
    ClientState, MembershipProof, PacketAttestation, PacketCommitment, StateAttestation,
};
use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;
use ibc_eureka_solidity_types::msgs::IAttestationMsgs;
use sha2::{Digest, Sha256};

const SIGNATURE_LEN: usize = 65;
const ETH_ADDRESS_LEN: usize = 20;
const KECCAK256_HASH_LEN: usize = 32;
const SECP256K1_PUBLIC_KEY_LENGTH: usize = 64;

/// Recovered public key from secp256k1_recover
#[derive(Debug, Clone, Copy)]
pub struct Secp256k1RecoveredPublicKey(pub [u8; SECP256K1_PUBLIC_KEY_LENGTH]);

// Define the secp256k1_recover syscall - only available on BPF target
#[cfg(target_os = "solana")]
solana_define_syscall::define_syscall!(fn sol_secp256k1_recover(hash: *const u8, recovery_id: u64, signature: *const u8, result: *mut u8) -> u64);

/// Recover secp256k1 public key from message hash and signature
#[cfg(target_os = "solana")]
fn secp256k1_recover(
    hash: &[u8; 32],
    recovery_id: u8,
    signature: &[u8; 64],
) -> core::result::Result<Secp256k1RecoveredPublicKey, ()> {
    let mut result = [0u8; SECP256K1_PUBLIC_KEY_LENGTH];

    let ret = unsafe {
        sol_secp256k1_recover(
            hash.as_ptr(),
            recovery_id as u64,
            signature.as_ptr(),
            result.as_mut_ptr(),
        )
    };

    if ret == 0 {
        Ok(Secp256k1RecoveredPublicKey(result))
    } else {
        Err(())
    }
}

/// Stub for non-BPF targets (native builds, IDL generation)
#[cfg(not(target_os = "solana"))]
fn secp256k1_recover(
    _hash: &[u8; 32],
    _recovery_id: u8,
    _signature: &[u8; 64],
) -> core::result::Result<Secp256k1RecoveredPublicKey, ()> {
    // This is only called during IDL generation or native tests
    // Return a dummy value - real verification happens on-chain
    Err(())
}

/// Recover Ethereum address from a signature using Solana's secp256k1_recover syscall
pub fn recover_eth_address(message: &[u8], signature: &[u8]) -> Result<[u8; ETH_ADDRESS_LEN]> {
    if signature.len() != SIGNATURE_LEN {
        return Err(error!(ErrorCode::InvalidSignature));
    }

    // Hash message with sha256 (matches the attestor signing approach)
    let message_hash: [u8; 32] = Sha256::digest(message).into();

    // Extract r, s, and v from signature (65 bytes: r[32] || s[32] || v[1])
    let recovery_id = signature[64];

    // Normalize recovery_id: Ethereum uses 27/28, Solana uses 0/1
    let recovery_id = if recovery_id >= 27 {
        recovery_id.saturating_sub(27)
    } else {
        recovery_id
    };

    // Extract signature (r || s)
    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(&signature[..64]);

    // Use Solana's secp256k1_recover syscall
    let recovered_pubkey =
        secp256k1_recover(&message_hash, recovery_id, &sig_bytes).map_err(|()| {
            msg!("secp256k1_recover failed");
            error!(ErrorCode::InvalidSignature)
        })?;

    // Convert public key to Ethereum address (keccak256 of pubkey, take last 20 bytes)
    let eth_address = pubkey_to_eth_address(&recovered_pubkey.0);

    Ok(eth_address)
}

/// Convert a 64-byte secp256k1 public key to Ethereum address
fn pubkey_to_eth_address(pubkey: &[u8; 64]) -> [u8; ETH_ADDRESS_LEN] {
    // Ethereum address = keccak256(pubkey)[12..32]
    let hash = keccak256(pubkey);
    let mut address = [0u8; ETH_ADDRESS_LEN];
    address.copy_from_slice(&hash[12..32]);
    address
}

/// Compute keccak256 hash
fn keccak256(data: &[u8]) -> [u8; KECCAK256_HASH_LEN] {
    solana_keccak_hasher::hash(data).0
}

/// Verify attestation signatures and return recovered addresses
pub fn verify_attestation(
    client_state: &ClientState,
    attestation_data: &[u8],
    raw_signatures: &[Vec<u8>],
) -> Result<()> {
    if raw_signatures.is_empty() {
        return Err(error!(ErrorCode::NoSignatures));
    }

    if raw_signatures.len() < client_state.min_required_sigs as usize {
        return Err(error!(ErrorCode::TooFewSignatures));
    }

    // Check for duplicate signatures by comparing signature bytes
    for (i, sig1) in raw_signatures.iter().enumerate() {
        for sig2 in raw_signatures.iter().skip(i.saturating_add(1)) {
            if sig1 == sig2 {
                return Err(error!(ErrorCode::DuplicateSignature));
            }
        }
    }

    // Verify each signature by recovering its address
    for raw_sig in raw_signatures {
        let recovered_address = recover_eth_address(attestation_data, raw_sig)?;

        // Check if the recovered address is in the trusted attestor set
        let is_trusted = client_state
            .attestor_addresses
            .iter()
            .any(|trusted_addr| *trusted_addr == recovered_address);

        if !is_trusted {
            return Err(error!(ErrorCode::UnknownAddressRecovered));
        }
    }

    Ok(())
}

/// Maximum allowed proof size to prevent allocation attacks
const MAX_PROOF_SIZE: usize = 64 * 1024; // 64 KB

/// Deserialize membership proof from borsh-encoded bytes
pub fn deserialize_membership_proof(proof_bytes: &[u8]) -> Result<MembershipProof> {
    // Validate size before deserialization to prevent allocation panics from malicious data
    if proof_bytes.len() > MAX_PROOF_SIZE {
        msg!(
            "Proof size {} exceeds maximum {}",
            proof_bytes.len(),
            MAX_PROOF_SIZE
        );
        return Err(error!(ErrorCode::InvalidProof));
    }

    MembershipProof::try_from_slice(proof_bytes).map_err(|e| {
        msg!("Failed to deserialize membership proof: {}", e);
        error!(ErrorCode::InvalidProof)
    })
}

/// ABI decode PacketAttestation from bytes using alloy
/// Matches Solidity: struct PacketAttestation { uint64 height; PacketCompact[] packets; }
pub fn decode_packet_attestation(data: &[u8]) -> Result<PacketAttestation> {
    let decoded = IAttestationMsgs::PacketAttestation::abi_decode(data)
        .map_err(|_| error!(ErrorCode::InvalidAttestationData))?;

    let packets = decoded
        .packets
        .iter()
        .map(|p| PacketCommitment {
            path: p.path.into(),
            commitment: p.commitment.into(),
        })
        .collect();

    Ok(PacketAttestation {
        height: decoded.height,
        packets,
    })
}

/// ABI decode StateAttestation from bytes using alloy
/// Matches Solidity: struct StateAttestation { uint64 height; uint64 timestamp; }
pub fn decode_state_attestation(data: &[u8]) -> Result<StateAttestation> {
    let decoded = IAttestationMsgs::StateAttestation::abi_decode(data)
        .map_err(|_| error!(ErrorCode::InvalidAttestationData))?;

    Ok(StateAttestation {
        height: decoded.height,
        timestamp: decoded.timestamp,
    })
}

/// Compute keccak256 hash of a path
pub fn hash_path(path: &[u8]) -> [u8; 32] {
    keccak256(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_client_state(
        attestor_addresses: Vec<[u8; 20]>,
        min_required_sigs: u8,
    ) -> ClientState {
        ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: "test-client".to_string(),
            attestor_addresses,
            min_required_sigs,
            latest_height: 100,
            is_frozen: false,
        }
    }

    fn create_test_signature(index: u8) -> Vec<u8> {
        let mut sig = vec![index; 64];
        sig.push(27); // recovery_id
        sig
    }

    // ==================== keccak256 tests ====================

    #[test]
    fn test_keccak256() {
        let data = b"hello";
        let hash = keccak256(data);
        assert_eq!(hash.len(), 32);
        // Known keccak256 hash of "hello"
        let expected =
            hex::decode("1c8aff950685c2ed4bc3174f3472287b56d9517b9c948127319a09a7a36deac8")
                .unwrap();
        assert_eq!(hash.as_slice(), expected.as_slice());
    }

    #[test]
    fn test_keccak256_empty() {
        let hash = keccak256(b"");
        // Known keccak256 hash of empty string
        let expected =
            hex::decode("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470")
                .unwrap();
        assert_eq!(hash.as_slice(), expected.as_slice());
    }

    // ==================== pubkey_to_eth_address tests ====================

    #[test]
    fn test_pubkey_to_eth_address() {
        let pubkey = [0u8; 64];
        let address = pubkey_to_eth_address(&pubkey);
        assert_eq!(address.len(), 20);
    }

    #[test]
    fn test_pubkey_to_eth_address_known_value() {
        // Use a deterministic pubkey and verify the address
        let mut pubkey = [0u8; 64];
        pubkey[0] = 0x04; // Typical uncompressed pubkey prefix byte position
        let address = pubkey_to_eth_address(&pubkey);
        assert_eq!(address.len(), 20);
        // Address should be last 20 bytes of keccak256(pubkey)
        let expected_hash = keccak256(&pubkey);
        assert_eq!(address, expected_hash[12..32]);
    }

    // ==================== hash_path tests ====================

    #[test]
    fn test_hash_path() {
        let path = b"ibc/commitments/channel-0/sequence/1";
        let hash = hash_path(path);
        assert_eq!(hash.len(), 32);
        // Should be same as keccak256
        assert_eq!(hash, keccak256(path));
    }

    #[test]
    fn test_hash_path_empty() {
        let hash = hash_path(b"");
        assert_eq!(hash, keccak256(b""));
    }

    // ==================== recover_eth_address tests ====================

    #[test]
    fn test_recover_eth_address_invalid_signature_length_short() {
        let message = b"test message";
        let short_sig = vec![0u8; 64]; // Missing recovery byte
        let result = recover_eth_address(message, &short_sig);
        assert!(result.is_err());
    }

    #[test]
    fn test_recover_eth_address_invalid_signature_length_long() {
        let message = b"test message";
        let long_sig = vec![0u8; 66]; // One byte too long
        let result = recover_eth_address(message, &long_sig);
        assert!(result.is_err());
    }

    #[test]
    fn test_recover_eth_address_empty_signature() {
        let message = b"test message";
        let empty_sig: Vec<u8> = vec![];
        let result = recover_eth_address(message, &empty_sig);
        assert!(result.is_err());
    }

    // ==================== verify_attestation tests ====================

    #[test]
    fn test_verify_attestation_no_signatures() {
        let client_state = create_test_client_state(vec![[1u8; 20]], 1);
        let attestation_data = b"test data";
        let signatures: Vec<Vec<u8>> = vec![];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
        // Should be NoSignatures error
    }

    #[test]
    fn test_verify_attestation_too_few_signatures() {
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20]], 2);
        let attestation_data = b"test data";
        let signatures = vec![create_test_signature(1)]; // Only 1, need 2

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
        // Should be TooFewSignatures error
    }

    #[test]
    fn test_verify_attestation_duplicate_signatures() {
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20]], 2);
        let attestation_data = b"test data";
        let sig = create_test_signature(1);
        let signatures = vec![sig.clone(), sig]; // Same signature twice

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
        // Should be DuplicateSignature error
    }

    #[test]
    fn test_verify_attestation_min_sigs_zero_with_no_sigs() {
        // Edge case: min_required_sigs = 0, but still need at least one signature
        let client_state = create_test_client_state(vec![[1u8; 20]], 0);
        let attestation_data = b"test data";
        let signatures: Vec<Vec<u8>> = vec![];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
        // Should be NoSignatures error (empty check happens first)
    }

    // ==================== deserialize_membership_proof tests ====================

    #[test]
    fn test_deserialize_membership_proof_valid() {
        let proof = MembershipProof {
            attestation_data: vec![1, 2, 3],
            signatures: vec![vec![4, 5, 6]],
        };
        let borsh_bytes = borsh::to_vec(&proof).unwrap();

        let result = deserialize_membership_proof(&borsh_bytes).unwrap();
        assert_eq!(result.attestation_data, vec![1, 2, 3]);
        assert_eq!(result.signatures, vec![vec![4, 5, 6]]);
    }

    #[test]
    fn test_deserialize_membership_proof_empty_fields() {
        let proof = MembershipProof {
            attestation_data: vec![],
            signatures: vec![],
        };
        let borsh_bytes = borsh::to_vec(&proof).unwrap();

        let result = deserialize_membership_proof(&borsh_bytes).unwrap();
        assert!(result.attestation_data.is_empty());
        assert!(result.signatures.is_empty());
    }

    #[test]
    fn test_deserialize_membership_proof_invalid_bytes() {
        let invalid_bytes = b"not valid borsh data";
        let result = deserialize_membership_proof(invalid_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_membership_proof_truncated() {
        let proof = MembershipProof {
            attestation_data: vec![1, 2, 3],
            signatures: vec![vec![4, 5, 6]],
        };
        let mut borsh_bytes = borsh::to_vec(&proof).unwrap();
        borsh_bytes.truncate(5); // Truncate to make invalid

        let result = deserialize_membership_proof(&borsh_bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_membership_proof_empty_bytes() {
        let empty: &[u8] = b"";
        let result = deserialize_membership_proof(empty);
        assert!(result.is_err());
    }

    // ==================== decode_packet_attestation tests ====================
    // PacketAttestation format with tuple wrapper (matches alloy's abi_encode):
    // tuple_offset (u256) || height (u256) || packets_rel_offset (u256) || [at offset: length || packets...]

    #[test]
    fn test_decode_packet_attestation() {
        let tuple_offset: u64 = 32;
        let height: u64 = 100;
        let packets_rel_offset: u64 = 64;

        let mut data = vec![0u8; 128]; // tuple_offset (32) + height (32) + rel_offset (32) + length (32)
        data[24..32].copy_from_slice(&tuple_offset.to_be_bytes());
        data[56..64].copy_from_slice(&height.to_be_bytes());
        data[88..96].copy_from_slice(&packets_rel_offset.to_be_bytes());
        // packets length = 0 at offset 96

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, height);
        assert!(result.packets.is_empty());
    }

    #[test]
    fn test_decode_packet_attestation_with_packets() {
        let tuple_offset: u64 = 32;
        let height: u64 = 100;
        let packets_rel_offset: u64 = 64;
        let packets_length: u64 = 1;

        let mut data = vec![0u8; 192]; // header (96) + length (32) + 1 packet (64)
        data[24..32].copy_from_slice(&tuple_offset.to_be_bytes());
        data[56..64].copy_from_slice(&height.to_be_bytes());
        data[88..96].copy_from_slice(&packets_rel_offset.to_be_bytes());
        data[120..128].copy_from_slice(&packets_length.to_be_bytes());
        data[128..160].copy_from_slice(&[1u8; 32]);
        data[160..192].copy_from_slice(&[2u8; 32]);

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, height);
        assert_eq!(result.packets.len(), 1);
        assert_eq!(result.packets[0].path, [1u8; 32]);
        assert_eq!(result.packets[0].commitment, [2u8; 32]);
    }

    #[test]
    fn test_decode_packet_attestation_multiple_packets() {
        let tuple_offset: u64 = 32;
        let height: u64 = 200;
        let packets_rel_offset: u64 = 64;
        let packets_length: u64 = 3;

        let mut data = vec![0u8; 128 + 64 * 3]; // header (128) + 3 packets
        data[24..32].copy_from_slice(&tuple_offset.to_be_bytes());
        data[56..64].copy_from_slice(&height.to_be_bytes());
        data[88..96].copy_from_slice(&packets_rel_offset.to_be_bytes());
        data[120..128].copy_from_slice(&packets_length.to_be_bytes());

        // Packet 0
        data[128..160].copy_from_slice(&[1u8; 32]);
        data[160..192].copy_from_slice(&[2u8; 32]);
        // Packet 1
        data[192..224].copy_from_slice(&[3u8; 32]);
        data[224..256].copy_from_slice(&[4u8; 32]);
        // Packet 2
        data[256..288].copy_from_slice(&[5u8; 32]);
        data[288..320].copy_from_slice(&[6u8; 32]);

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, height);
        assert_eq!(result.packets.len(), 3);
        assert_eq!(result.packets[0].path, [1u8; 32]);
        assert_eq!(result.packets[1].path, [3u8; 32]);
        assert_eq!(result.packets[2].path, [5u8; 32]);
    }

    #[test]
    fn test_decode_packet_attestation_too_short() {
        // Very short data that cannot possibly be a valid ABI-encoded PacketAttestation
        let data = vec![0u8; 16];
        let result = decode_packet_attestation(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_packet_attestation_empty() {
        let data: Vec<u8> = vec![];
        let result = decode_packet_attestation(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_packet_attestation_invalid_offset() {
        let tuple_offset: u64 = 32;
        let height: u64 = 100;
        let packets_rel_offset: u64 = 1000; // Points beyond data

        let mut data = vec![0u8; 128];
        data[24..32].copy_from_slice(&tuple_offset.to_be_bytes());
        data[56..64].copy_from_slice(&height.to_be_bytes());
        data[88..96].copy_from_slice(&packets_rel_offset.to_be_bytes());

        let result = decode_packet_attestation(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_packet_attestation_truncated_packets() {
        let tuple_offset: u64 = 32;
        let height: u64 = 100;
        let packets_rel_offset: u64 = 64;
        let packets_length: u64 = 2; // Claims 2 packets

        let mut data = vec![0u8; 192]; // Only enough for 1 packet
        data[24..32].copy_from_slice(&tuple_offset.to_be_bytes());
        data[56..64].copy_from_slice(&height.to_be_bytes());
        data[88..96].copy_from_slice(&packets_rel_offset.to_be_bytes());
        data[120..128].copy_from_slice(&packets_length.to_be_bytes());
        data[128..160].copy_from_slice(&[1u8; 32]);
        data[160..192].copy_from_slice(&[2u8; 32]);

        let result = decode_packet_attestation(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_packet_attestation_zero_height() {
        let tuple_offset: u64 = 32;
        let height: u64 = 0;
        let packets_rel_offset: u64 = 64;

        let mut data = vec![0u8; 128];
        data[24..32].copy_from_slice(&tuple_offset.to_be_bytes());
        data[56..64].copy_from_slice(&height.to_be_bytes());
        data[88..96].copy_from_slice(&packets_rel_offset.to_be_bytes());

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 0);
    }

    #[test]
    fn test_decode_packet_attestation_max_values() {
        let tuple_offset: u64 = 32;
        let height: u64 = u64::MAX;
        let packets_rel_offset: u64 = 64;

        let mut data = vec![0u8; 128];
        data[24..32].copy_from_slice(&tuple_offset.to_be_bytes());
        data[56..64].copy_from_slice(&height.to_be_bytes());
        data[88..96].copy_from_slice(&packets_rel_offset.to_be_bytes());

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, u64::MAX);
    }

    // ==================== verify_attestation additional tests ====================

    #[test]
    fn test_verify_attestation_different_signatures() {
        // Test that different signatures are allowed (no duplicate check triggers)
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20]], 2);
        let attestation_data = b"test data";
        let sig1 = create_test_signature(1);
        let sig2 = create_test_signature(2); // Different from sig1

        let result = verify_attestation(&client_state, attestation_data, &[sig1, sig2]);
        // Will fail at signature recovery (stub returns error), but not at duplicate check
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_exact_required_sigs() {
        // Test with exactly the required number of signatures
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 2);
        let attestation_data = b"test data";
        let signatures = vec![create_test_signature(1), create_test_signature(2)];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        // Will fail at signature recovery, but passes the count check
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_more_than_required_sigs() {
        // Test with more signatures than required
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 2);
        let attestation_data = b"test data";
        let signatures = vec![
            create_test_signature(1),
            create_test_signature(2),
            create_test_signature(3),
        ];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        // Will fail at signature recovery, but passes the count check
        assert!(result.is_err());
    }

    // ==================== hash_path additional tests ====================

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
}
