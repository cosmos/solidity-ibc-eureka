use crate::error::ErrorCode;
use anchor_lang::prelude::*;

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

// TODO: CRITICAL - Implement SHA256 digest computation
// The Solidity implementation uses sha256() to hash the attestationData before signature verification.
// This is required for ECDSA signature verification in all proof validation flows.
// See: AttestationLightClient.sol:97, 141, 180
// Example implementation:
// pub fn sha256_digest(data: &[u8]) -> [u8; 32] {
//     use solana_program::hash::hashv;
//     hashv(&[data]).to_bytes()
// }

// TODO: CRITICAL - Implement verify_signatures_threshold function
// This function must verify that the provided signatures meet the attestor threshold.
// It should replicate the logic from AttestationLightClient.sol:214-233:
//
// Parameters:
// - digest: [u8; 32] - SHA256 hash of attestation_data
// - signatures: Vec<Vec<u8>> - Array of 65-byte ECDSA signatures (r||s||v format)
// - attestor_addresses: &Vec<[u8; 20]> - Known attestor set
// - min_required_sigs: u8 - Minimum signatures needed
//
// Logic:
// 1. Verify signatures.len() >= min_required_sigs
// 2. For each signature:
//    a. Verify signature length is exactly 65 bytes (ECDSA_SIGNATURE_LENGTH)
//    b. Recover signer address using ECDSA recovery (secp256k1_recover)
//    c. Verify recovered address is in attestor_addresses set
//    d. Check for duplicate signers (track seen addresses)
// 3. Return Ok(()) if all checks pass, Err otherwise
//
// Solana-specific notes:
// - Use solana_program::secp256k1_recover for ECDSA recovery
// - Ethereum addresses are last 20 bytes of keccak256(pubkey)
// - The digest needs to be prefixed with Ethereum's "\x19Ethereum Signed Message:\n32" for ecrecover compatibility
// - Consider using k256 or libsecp256k1 crates for signature verification
//
// See: contracts/light-clients/attestation/AttestationLightClient.sol:214-233

// TODO: CRITICAL - Implement verify_signature helper function
// This function should verify a single ECDSA signature and return the recovered signer address.
// It should replicate the logic from AttestationLightClient.sol:240-248:
//
// Parameters:
// - digest: [u8; 32] - SHA256 hash of attestation_data
// - signature: &[u8] - 65-byte ECDSA signature (r||s||v format)
// - attestor_addresses: &Vec<[u8; 20]> - Known attestor set for validation
//
// Logic:
// 1. Verify signature.len() == 65 (ECDSA_SIGNATURE_LENGTH constant)
// 2. Perform ECDSA recovery to get public key:
//    a. Extract r, s, v from signature (first 32 bytes = r, next 32 = s, last byte = v)
//    b. Use secp256k1_recover with digest and signature
// 3. Derive Ethereum address from recovered public key:
//    a. Take keccak256 hash of the uncompressed public key (64 bytes, without 0x04 prefix)
//    b. Take last 20 bytes as Ethereum address
// 4. Verify recovered address != [0; 20] (invalid signature)
// 5. Verify recovered address is in attestor_addresses (known signer)
// 6. Return recovered address
//
// Errors:
// - InvalidSignatureLength if len != 65
// - InvalidSignature if recovery fails or address is zero
// - UnknownSigner if recovered address not in attestor set
//
// See: contracts/light-clients/attestation/AttestationLightClient.sol:240-248

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
