//! Manual ABI decoding for Ethereum-compatible attestation types.
//!
//! This module provides manual implementations of Ethereum ABI decoding for attestation
//! structs used in the IBC protocol. We implement these decoders manually instead of using
//! the `alloy-sol-types` crate for the following reasons:
//!
//! 1. **Solana toolchain compatibility**: The Solana toolchain bundles Cargo 1.84, which
//!    does not support Rust edition 2024. The `alloy` crates (v1.2+) use edition 2024,
//!    causing build failures during `anchor build` when the toolchain's internal cargo
//!    is used for running tests.
//!
//! 2. **Minimal dependency footprint**: By implementing only the specific decoders we need,
//!    we avoid pulling in the entire alloy dependency tree, reducing compile times and
//!    binary size.
//!
//! 3. **Predictable behavior**: Manual implementation gives us full control over error
//!    handling and validation without relying on external crate behavior.
//!
//! The ABI encoding format follows the Ethereum ABI specification where:
//! - All values are padded to 32-byte words
//! - Integers are big-endian and right-aligned within the word
//! - Dynamic arrays use offset pointers

use crate::error::ErrorCode;
use crate::types::{PacketAttestation, PacketCommitment, StateAttestation};
use anchor_lang::prelude::*;

const ABI_WORD_SIZE: usize = 32;

/// Read a u64 from a 32-byte ABI-encoded word.
///
/// In ABI encoding, integers smaller than 256 bits are right-aligned (big-endian)
/// within the 32-byte word. For u64, the value occupies the last 8 bytes.
fn read_u64_from_word(data: &[u8], offset: usize) -> Option<u64> {
    if offset.saturating_add(ABI_WORD_SIZE) > data.len() {
        return None;
    }
    let start = offset.saturating_add(24);
    let bytes: [u8; 8] = data[start..start.saturating_add(8)].try_into().ok()?;
    Some(u64::from_be_bytes(bytes))
}

/// Read a bytes32 from ABI-encoded data.
fn read_bytes32(data: &[u8], offset: usize) -> Option<[u8; 32]> {
    if offset.saturating_add(32) > data.len() {
        return None;
    }
    data[offset..offset.saturating_add(32)].try_into().ok()
}

/// Decode a `PacketAttestation` from ABI-encoded bytes.
///
/// Corresponds to the Solidity struct:
/// ```solidity
/// struct PacketAttestation {
///     uint64 height;
///     PacketCompact[] packets;
/// }
///
/// struct PacketCompact {
///     bytes32 path;
///     bytes32 commitment;
/// }
/// ```
///
/// ABI encoding layout (with tuple wrapper from `abi.encode`):
/// - `[0..32]`: tuple offset (always 32, pointing to struct start)
/// - `[32..64]`: height (u256, u64 value in last 8 bytes)
/// - `[64..96]`: relative offset to packets array (from struct start)
/// - `[96..128]`: packets array length
/// - `[128..]`: packets data (64 bytes each: path || commitment)
pub fn decode_packet_attestation(data: &[u8]) -> Result<PacketAttestation> {
    const MIN_SIZE: usize = 128;

    if data.len() < MIN_SIZE {
        return Err(error!(ErrorCode::InvalidAttestationData));
    }

    let tuple_offset = read_u64_from_word(data, 0)
        .ok_or_else(|| error!(ErrorCode::InvalidAttestationData))? as usize;

    if tuple_offset != 32 {
        return Err(error!(ErrorCode::InvalidAttestationData));
    }

    let height =
        read_u64_from_word(data, 32).ok_or_else(|| error!(ErrorCode::InvalidAttestationData))?;

    let packets_rel_offset = read_u64_from_word(data, 64)
        .ok_or_else(|| error!(ErrorCode::InvalidAttestationData))?
        as usize;

    // Absolute offset to packets length (relative offset is from struct start at byte 32)
    let packets_length_offset = 32_usize.saturating_add(packets_rel_offset);

    if packets_length_offset.saturating_add(32) > data.len() {
        return Err(error!(ErrorCode::InvalidAttestationData));
    }

    let packets_len = read_u64_from_word(data, packets_length_offset)
        .ok_or_else(|| error!(ErrorCode::InvalidAttestationData))? as usize;

    let packets_data_offset = packets_length_offset.saturating_add(32);
    let required_len = packets_data_offset.saturating_add(packets_len.saturating_mul(64));

    if required_len > data.len() {
        return Err(error!(ErrorCode::InvalidAttestationData));
    }

    let mut packets = Vec::with_capacity(packets_len);
    for i in 0..packets_len {
        let packet_offset = packets_data_offset.saturating_add(i.saturating_mul(64));
        let path = read_bytes32(data, packet_offset)
            .ok_or_else(|| error!(ErrorCode::InvalidAttestationData))?;
        let commitment = read_bytes32(data, packet_offset.saturating_add(32))
            .ok_or_else(|| error!(ErrorCode::InvalidAttestationData))?;
        packets.push(PacketCommitment { path, commitment });
    }

    Ok(PacketAttestation { height, packets })
}

/// Decode a `StateAttestation` from ABI-encoded bytes.
///
/// Corresponds to the Solidity struct:
/// ```solidity
/// struct StateAttestation {
///     uint64 height;
///     uint64 timestamp;
/// }
/// ```
///
/// ABI encoding layout:
/// - `[0..32]`: height (u256, u64 value in last 8 bytes)
/// - `[32..64]`: timestamp (u256, u64 value in last 8 bytes)
pub fn decode_state_attestation(data: &[u8]) -> Result<StateAttestation> {
    const MIN_SIZE: usize = 64;

    if data.len() < MIN_SIZE {
        return Err(error!(ErrorCode::InvalidAttestationData));
    }

    let height =
        read_u64_from_word(data, 0).ok_or_else(|| error!(ErrorCode::InvalidAttestationData))?;

    let timestamp =
        read_u64_from_word(data, 32).ok_or_else(|| error!(ErrorCode::InvalidAttestationData))?;

    Ok(StateAttestation { height, timestamp })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_u256(value: u64) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&value.to_be_bytes());
        bytes
    }

    // ==================== StateAttestation tests ====================

    #[test]
    fn test_decode_state_attestation_basic() {
        let height: u64 = 100;
        let timestamp: u64 = 1_700_000_000;

        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(height));
        data.extend_from_slice(&encode_u256(timestamp));

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, height);
        assert_eq!(result.timestamp, timestamp);
    }

    #[test]
    fn test_decode_state_attestation_zero_values() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(0));
        data.extend_from_slice(&encode_u256(0));

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 0);
        assert_eq!(result.timestamp, 0);
    }

    #[test]
    fn test_decode_state_attestation_max_values() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(u64::MAX));
        data.extend_from_slice(&encode_u256(u64::MAX));

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, u64::MAX);
        assert_eq!(result.timestamp, u64::MAX);
    }

    #[test]
    fn test_decode_state_attestation_too_short() {
        let data = vec![0u8; 63];
        assert!(decode_state_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_state_attestation_empty() {
        let data: Vec<u8> = vec![];
        assert!(decode_state_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_state_attestation_extra_data_ignored() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(42));
        data.extend_from_slice(&encode_u256(123));
        data.extend_from_slice(&[0xffu8; 64]); // Extra garbage data

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 42);
        assert_eq!(result.timestamp, 123);
    }

    #[test]
    fn test_decode_state_attestation_realistic_values() {
        // Realistic block height and Unix timestamp
        let height: u64 = 18_500_000; // Ethereum mainnet block
        let timestamp: u64 = 1_700_000_000; // Nov 2023

        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(height));
        data.extend_from_slice(&encode_u256(timestamp));

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, height);
        assert_eq!(result.timestamp, timestamp);
    }

    // ==================== PacketAttestation tests ====================

    #[test]
    fn test_decode_packet_attestation_empty_packets() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32)); // tuple offset
        data.extend_from_slice(&encode_u256(100)); // height
        data.extend_from_slice(&encode_u256(64)); // packets rel offset
        data.extend_from_slice(&encode_u256(0)); // packets length

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 100);
        assert!(result.packets.is_empty());
    }

    #[test]
    fn test_decode_packet_attestation_single_packet() {
        let path = [0xabu8; 32];
        let commitment = [0xcdu8; 32];

        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(500));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(1));
        data.extend_from_slice(&path);
        data.extend_from_slice(&commitment);

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 500);
        assert_eq!(result.packets.len(), 1);
        assert_eq!(result.packets[0].path, path);
        assert_eq!(result.packets[0].commitment, commitment);
    }

    #[test]
    fn test_decode_packet_attestation_multiple_packets() {
        let packets_data: Vec<([u8; 32], [u8; 32])> = vec![
            ([1u8; 32], [2u8; 32]),
            ([3u8; 32], [4u8; 32]),
            ([5u8; 32], [6u8; 32]),
        ];

        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(999));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(packets_data.len() as u64));
        for (path, commitment) in &packets_data {
            data.extend_from_slice(path);
            data.extend_from_slice(commitment);
        }

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 999);
        assert_eq!(result.packets.len(), 3);
        for (i, (path, commitment)) in packets_data.iter().enumerate() {
            assert_eq!(result.packets[i].path, *path);
            assert_eq!(result.packets[i].commitment, *commitment);
        }
    }

    #[test]
    fn test_decode_packet_attestation_zero_height() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(0));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 0);
    }

    #[test]
    fn test_decode_packet_attestation_max_height() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(u64::MAX));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, u64::MAX);
    }

    #[test]
    fn test_decode_packet_attestation_too_short() {
        let data = vec![0u8; 127];
        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_empty() {
        let data: Vec<u8> = vec![];
        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_invalid_tuple_offset_zero() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(0)); // Invalid: should be 32
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_invalid_tuple_offset_wrong_value() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(64)); // Invalid: should be 32
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_packets_offset_out_of_bounds() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(1000)); // Points beyond data
        data.extend_from_slice(&encode_u256(0));

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_truncated_packets_array() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(3)); // Claims 3 packets
        data.extend_from_slice(&[1u8; 32]); // Only 1 packet
        data.extend_from_slice(&[2u8; 32]);

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_partial_packet() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(1));
        data.extend_from_slice(&[1u8; 32]); // Only path, missing commitment

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_extra_data_ignored() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(42));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));
        data.extend_from_slice(&[0xffu8; 128]); // Extra garbage

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 42);
        assert!(result.packets.is_empty());
    }

    #[test]
    fn test_decode_packet_attestation_many_packets() {
        let packet_count = 100;
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(12345));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(packet_count));

        for i in 0..packet_count {
            let mut path = [0u8; 32];
            path[0] = i as u8;
            let mut commitment = [0u8; 32];
            commitment[31] = i as u8;
            data.extend_from_slice(&path);
            data.extend_from_slice(&commitment);
        }

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 12345);
        assert_eq!(result.packets.len(), packet_count as usize);

        for i in 0..packet_count as usize {
            assert_eq!(result.packets[i].path[0], i as u8);
            assert_eq!(result.packets[i].commitment[31], i as u8);
        }
    }

    #[test]
    fn test_decode_packet_attestation_distinct_packet_values() {
        let path1 = {
            let mut p = [0u8; 32];
            p[0..4].copy_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
            p
        };
        let commitment1 = {
            let mut c = [0u8; 32];
            c[28..32].copy_from_slice(&[0xca, 0xfe, 0xba, 0xbe]);
            c
        };

        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(777));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(1));
        data.extend_from_slice(&path1);
        data.extend_from_slice(&commitment1);

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.packets[0].path[0..4], [0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(
            result.packets[0].commitment[28..32],
            [0xca, 0xfe, 0xba, 0xbe]
        );
    }

    // ==================== Edge cases for internal helpers ====================

    #[test]
    fn test_decode_packet_attestation_exactly_minimum_size() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(1));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));
        assert_eq!(data.len(), 128);

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 1);
        assert!(result.packets.is_empty());
    }

    #[test]
    fn test_decode_state_attestation_exactly_minimum_size() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(1));
        data.extend_from_slice(&encode_u256(2));
        assert_eq!(data.len(), 64);

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 1);
        assert_eq!(result.timestamp, 2);
    }

    // ==================== Overflow and boundary tests ====================

    #[test]
    fn test_decode_packet_attestation_huge_packet_count_overflow() {
        // Test that a malicious huge packet count doesn't cause overflow
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(u64::MAX)); // Impossibly large count

        // Should fail gracefully due to bounds check, not panic from overflow
        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_large_offset_overflow() {
        // Test that a huge relative offset doesn't cause overflow
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(u64::MAX)); // Huge offset
        data.extend_from_slice(&encode_u256(0));

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_state_attestation_non_zero_high_bytes() {
        // ABI encodes u64 as u256 with zero high bytes, but we should handle
        // non-zero high bytes gracefully (they're ignored, only last 8 bytes matter)
        let mut data = vec![0xffu8; 64]; // All 0xff
                                         // Set the actual u64 values in the last 8 bytes of each word
        data[24..32].copy_from_slice(&42u64.to_be_bytes());
        data[56..64].copy_from_slice(&123u64.to_be_bytes());

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 42);
        assert_eq!(result.timestamp, 123);
    }

    #[test]
    fn test_decode_packet_attestation_non_zero_high_bytes_in_height() {
        // Height word has non-zero high bytes (ignored)
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));

        // Height with garbage in high bytes
        let mut height_word = [0xffu8; 32];
        height_word[24..32].copy_from_slice(&999u64.to_be_bytes());
        data.extend_from_slice(&height_word);

        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 999);
    }

    #[test]
    fn test_decode_packet_attestation_one_byte_short_for_header() {
        // 127 bytes - exactly 1 byte short of minimum
        let data = vec![0u8; 127];
        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_state_attestation_one_byte_short() {
        // 63 bytes - exactly 1 byte short of minimum
        let data = vec![0u8; 63];
        assert!(decode_state_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_one_byte_short_for_packet() {
        // Header is valid, but packet data is 1 byte short
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(1)); // Claims 1 packet
        data.extend_from_slice(&[0u8; 63]); // 63 bytes instead of 64

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_offset_causes_wrap() {
        // Offset that when added to base could wrap around
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        // Use a value that's large but not u64::MAX
        data.extend_from_slice(&encode_u256((usize::MAX - 16) as u64));
        data.extend_from_slice(&encode_u256(0));

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_packet_count_causes_multiplication_overflow() {
        // Packet count that when multiplied by 64 would overflow
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        // usize::MAX / 64 + 1 would overflow when multiplied by 64
        let overflow_count = (usize::MAX / 64).saturating_add(1) as u64;
        data.extend_from_slice(&encode_u256(overflow_count));

        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_all_zeros() {
        // Completely zero data (invalid tuple offset)
        let data = vec![0u8; 128];
        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_state_attestation_all_zeros() {
        // Completely zero data (valid - height=0, timestamp=0)
        let data = vec![0u8; 64];
        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 0);
        assert_eq!(result.timestamp, 0);
    }

    #[test]
    fn test_decode_packet_attestation_all_ones() {
        // All 0xff bytes
        let mut data = vec![0xffu8; 128];
        // Fix tuple offset to be 32
        data[0..32].copy_from_slice(&encode_u256(32));

        // Will fail because offset 0xffffffffffffffff is out of bounds
        assert!(decode_packet_attestation(&data).is_err());
    }

    #[test]
    fn test_decode_packet_attestation_valid_with_zero_path_and_commitment() {
        // Packets with all-zero path and commitment are valid
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(1));
        data.extend_from_slice(&[0u8; 32]); // zero path
        data.extend_from_slice(&[0u8; 32]); // zero commitment

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.packets.len(), 1);
        assert_eq!(result.packets[0].path, [0u8; 32]);
        assert_eq!(result.packets[0].commitment, [0u8; 32]);
    }

    #[test]
    fn test_decode_packet_attestation_packets_length_at_boundary() {
        // Packets length field exactly at data boundary
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));
        data.extend_from_slice(&encode_u256(100));
        data.extend_from_slice(&encode_u256(64)); // Points to byte 96
                                                  // Data ends at 96, so reading length at 96 should fail
                                                  // Total: 96 bytes, need 128 minimum

        assert!(decode_packet_attestation(&data).is_err());
    }
}
