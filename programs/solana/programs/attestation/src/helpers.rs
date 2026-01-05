use crate::error::ErrorCode;
use crate::state::EthereumAddress;
use anchor_lang::prelude::*;
use sha2::{Digest, Sha256};
use solana_secp256k1_recover::secp256k1_recover;

/// ECDSA signature length (r || s || v)
const ECDSA_SIGNATURE_LENGTH: usize = 65;

/// Attestation data structures (matching Solidity ABI encoding)
#[derive(Debug, Clone)]
pub struct StateAttestation {
    pub height: u64,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct PacketAttestation {
    pub height: u64,
    pub packets: Vec<PacketCompact>,
}

#[derive(Debug, Clone)]
pub struct PacketCompact {
    pub path: [u8; 32],
    pub commitment: [u8; 32],
}

/// Attestation proof structure (from relayer)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AttestationProof {
    pub attestation_data: Vec<u8>,
    pub signatures: Vec<Vec<u8>>,
}

/// Decode StateAttestation from ABI-encoded bytes
/// Format: abi.encode(height: uint64, timestamp: uint64)
pub fn decode_state_attestation(data: &[u8]) -> Result<StateAttestation> {
    // Simple ABI decoding for (uint64, uint64)
    // ABI encoding pads each uint64 to 32 bytes
    if data.len() != 64 {
        return Err(ErrorCode::AbiDecodingFailed.into());
    }

    // Extract height (bytes 24-32 of first 32-byte word)
    let height = u64::from_be_bytes(
        data[24..32]
            .try_into()
            .map_err(|_| ErrorCode::AbiDecodingFailed)?,
    );

    // Extract timestamp (bytes 56-64 of second 32-byte word)
    let timestamp = u64::from_be_bytes(
        data[56..64]
            .try_into()
            .map_err(|_| ErrorCode::AbiDecodingFailed)?,
    );

    Ok(StateAttestation { height, timestamp })
}

/// Decode PacketAttestation from ABI-encoded bytes
/// Format: abi.encode(height: uint64, packets: PacketCompact[])
pub fn decode_packet_attestation(data: &[u8]) -> Result<PacketAttestation> {
    if data.len() < 96 {
        // Minimum: height (32) + array offset (32) + array length (32)
        return Err(ErrorCode::AbiDecodingFailed.into());
    }

    // Extract height (bytes 24-32 of first 32-byte word)
    let height = u64::from_be_bytes(
        data[24..32]
            .try_into()
            .map_err(|_| ErrorCode::AbiDecodingFailed)?,
    );

    // Extract array offset (bytes 32-64, should be 64 for dynamic arrays)
    let array_offset = u64::from_be_bytes(
        data[56..64]
            .try_into()
            .map_err(|_| ErrorCode::AbiDecodingFailed)?,
    ) as usize;

    if array_offset != 64 {
        return Err(ErrorCode::AbiDecodingFailed.into());
    }

    // Extract array length (bytes 88-96)
    let array_length = u64::from_be_bytes(
        data[88..96]
            .try_into()
            .map_err(|_| ErrorCode::AbiDecodingFailed)?,
    ) as usize;

    let mut packets = Vec::with_capacity(array_length);

    // Each PacketCompact is 64 bytes (2 * bytes32)
    let packets_start = 96;
    for i in 0..array_length {
        let packet_offset = packets_start + (i * 64);
        if data.len() < packet_offset + 64 {
            return Err(ErrorCode::AbiDecodingFailed.into());
        }

        let path: [u8; 32] = data[packet_offset..packet_offset + 32]
            .try_into()
            .map_err(|_| ErrorCode::AbiDecodingFailed)?;

        let commitment: [u8; 32] = data[packet_offset + 32..packet_offset + 64]
            .try_into()
            .map_err(|_| ErrorCode::AbiDecodingFailed)?;

        packets.push(PacketCompact { path, commitment });
    }

    Ok(PacketAttestation { height, packets })
}

/// Compute keccak256 hash of data
pub fn keccak256(data: &[u8]) -> [u8; 32] {
    solana_keccak_hasher::hash(data).to_bytes()
}

/// Compute SHA256
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Verifies a single ECDSA signature and returns the recovered Ethereum address.
///
/// # Arguments
/// * `digest` - SHA256 hash of attestation_data
/// * `signature` - 65-byte ECDSA signature (r||s||v format)
fn verify_signature(digest: &[u8; 32], signature: &[u8]) -> Result<EthereumAddress> {
    // Verify signature length
    require!(
        signature.len() == ECDSA_SIGNATURE_LENGTH,
        ErrorCode::InvalidSignature
    );

    // Extract r, s, v from signature (r: [0..32], s: [32..64], v: [64])
    // Note: Ethereum uses v = 27 or 28, but secp256k1_recover expects recovery_id = 0 or 1
    let recovery_id = signature[64]
        .checked_sub(27)
        .ok_or(ErrorCode::InvalidSignature)?;

    // Prepare the signature for secp256k1_recover (r || s format, 64 bytes)
    let sig_bytes: [u8; 64] = signature[0..64]
        .try_into()
        .map_err(|_| ErrorCode::InvalidSignature)?;

    // Recover the public key using Solana's secp256k1_recover syscall
    let recovered_pubkey = secp256k1_recover(digest, recovery_id, &sig_bytes)
        .map_err(|_| ErrorCode::InvalidSignature)?;

    // TODO: Should we use alloy types here?

    // Derive Ethereum address: keccak256(pubkey)[12..32]
    // The recovered pubkey is 64 bytes (uncompressed, without 0x04 prefix)
    let pubkey_hash = keccak256(&recovered_pubkey.to_bytes());
    let addr_bytes: [u8; 20] = pubkey_hash[12..32]
        .try_into()
        .map_err(|_| ErrorCode::InvalidSignature)?;
    let recovered_address = EthereumAddress::from(addr_bytes);

    // Verify the address is not zero (invalid signature)
    require!(
        recovered_address.0 != [0u8; 20],
        ErrorCode::InvalidSignature
    );

    Ok(recovered_address)
}

/// Verifies that `signatures` over `digest` are valid, unique, and meet the threshold.
///
/// # Arguments
/// * `digest` - SHA256 hash of attestation_data
/// * `signatures` - Array of 65-byte ECDSA signatures (r||s||v format)
/// * `attestor_addresses` - Known attestor set
/// * `min_required_sigs` - Minimum signatures needed
pub fn verify_signatures_threshold(
    digest: [u8; 32],
    signatures: &Vec<Vec<u8>>,
    attestor_addresses: &[EthereumAddress],
    min_required_sigs: u8,
) -> Result<()> {
    // Verify we have at least one signature
    require!(!signatures.is_empty(), ErrorCode::EmptySignatures);

    // Verify we meet the threshold
    require!(
        signatures.len() >= min_required_sigs as usize,
        ErrorCode::ThresholdNotMet
    );

    // Track seen addresses to detect duplicates
    let mut seen_addresses = Vec::with_capacity(signatures.len());

    for signature in signatures {
        // Verify the signature and recover the signer address
        let recovered = verify_signature(&digest, signature)?;

        // Verify the recovered address is in the attestor set
        require!(
            attestor_addresses.contains(&recovered),
            ErrorCode::UnknownSigner
        );

        // Check for duplicate signers
        require!(
            !seen_addresses.contains(&recovered),
            ErrorCode::DuplicateSignature
        );

        seen_addresses.push(recovered);
    }

    Ok(())
}

// TODO: Re-enable after fixing alloy dependency compatibility issues
// /// Convert attestor-light-client error to Anchor error
// pub fn convert_attestor_error(
//     err: attestor_light_client::error::IbcAttestorClientError,
// ) -> Error {
//     match err {
//         attestor_light_client::error::IbcAttestorClientError::ClientFrozen => {
//             ErrorCode::ClientFrozen.into()
//         }
//         attestor_light_client::error::IbcAttestorClientError::InvalidSignature => {
//             ErrorCode::InvalidSignature.into()
//         }
//         attestor_light-client::error::IbcAttestorClientError::UnknownAddressRecovered { .. } => {
//             ErrorCode::UnknownSigner.into()
//         }
//         attestor_light_client::error::IbcAttestorClientError::InvalidAttestedData { .. } => {
//             ErrorCode::AttestationVerificationFailed.into()
//         }
//         _ => ErrorCode::AttestationVerificationFailed.into(),
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_state_attestation() {
        // Create ABI-encoded StateAttestation(height=100, timestamp=1234567890)
        let mut data = vec![0u8; 64];

        // Height = 100 (padded to 32 bytes)
        data[31] = 100;

        // Timestamp = 1234567890 (0x499602D2) (padded to 32 bytes)
        data[60] = 0x49;
        data[61] = 0x96;
        data[62] = 0x02;
        data[63] = 0xD2;

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 100);
        assert_eq!(result.timestamp, 1234567890);
    }

    #[test]
    fn test_decode_state_attestation_invalid_length() {
        let data = vec![0u8; 32]; // Too short
        assert!(decode_state_attestation(&data).is_err());
    }

    #[test]
    fn test_keccak256() {
        let data = b"hello world";
        let hash = keccak256(data);
        assert_eq!(hash.len(), 32);
        // Verify it's not all zeros
        assert!(hash.iter().any(|&b| b != 0));
    }
}
