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
    use rstest::rstest;

    fn encode_u256(value: u64) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[24..32].copy_from_slice(&value.to_be_bytes());
        bytes
    }

    /// Build a standard packet attestation header (tuple_offset, height, packets_rel_offset, packets_len)
    fn build_packet_header(
        tuple_offset: u64,
        height: u64,
        packets_rel_offset: u64,
        packets_len: u64,
    ) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(tuple_offset));
        data.extend_from_slice(&encode_u256(height));
        data.extend_from_slice(&encode_u256(packets_rel_offset));
        data.extend_from_slice(&encode_u256(packets_len));
        data
    }

    // ==================== StateAttestation success tests ====================

    #[rstest]
    #[case::basic(100, 1_700_000_000)]
    #[case::zero_values(0, 0)]
    #[case::max_values(u64::MAX, u64::MAX)]
    #[case::realistic_values(18_500_000, 1_700_000_000)]
    #[case::exactly_minimum_size(1, 2)]
    fn test_decode_state_attestation_ok(#[case] height: u64, #[case] timestamp: u64) {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(height));
        data.extend_from_slice(&encode_u256(timestamp));

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, height);
        assert_eq!(result.timestamp, timestamp);
    }

    #[rstest]
    #[case::too_short(vec![0u8; 63])]
    #[case::empty(vec![])]
    #[case::one_byte_short(vec![0u8; 63])]
    fn test_decode_state_attestation_err(#[case] data: Vec<u8>) {
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
    fn test_decode_state_attestation_non_zero_high_bytes() {
        let mut data = vec![0xffu8; 64];
        data[24..32].copy_from_slice(&42u64.to_be_bytes());
        data[56..64].copy_from_slice(&123u64.to_be_bytes());

        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 42);
        assert_eq!(result.timestamp, 123);
    }

    #[test]
    fn test_decode_state_attestation_all_zeros() {
        let data = vec![0u8; 64];
        let result = decode_state_attestation(&data).unwrap();
        assert_eq!(result.height, 0);
        assert_eq!(result.timestamp, 0);
    }

    // ==================== PacketAttestation error tests ====================

    fn build_packet_err_truncated_array() -> Vec<u8> {
        let mut data = build_packet_header(32, 100, 64, 3);
        data.extend_from_slice(&[1u8; 32]); // Only 1 partial packet
        data.extend_from_slice(&[2u8; 32]);
        data
    }

    fn build_packet_err_partial_packet() -> Vec<u8> {
        let mut data = build_packet_header(32, 100, 64, 1);
        data.extend_from_slice(&[1u8; 32]); // Only path, missing commitment
        data
    }

    fn build_packet_err_one_byte_short_for_packet() -> Vec<u8> {
        let mut data = build_packet_header(32, 100, 64, 1);
        data.extend_from_slice(&[0u8; 63]); // 63 bytes instead of 64
        data
    }

    fn build_packet_err_all_ones() -> Vec<u8> {
        let mut data = vec![0xffu8; 128];
        data[0..32].copy_from_slice(&encode_u256(32));
        data
    }

    fn build_packet_err_multiplication_overflow() -> Vec<u8> {
        let overflow_count = (usize::MAX / 64).saturating_add(1) as u64;
        build_packet_header(32, 100, 64, overflow_count)
    }

    #[rstest]
    #[case::too_short(vec![0u8; 127])]
    #[case::empty(vec![])]
    #[case::one_byte_short_for_header(vec![0u8; 127])]
    #[case::invalid_tuple_offset_zero(build_packet_header(0, 100, 64, 0))]
    #[case::invalid_tuple_offset_wrong(build_packet_header(64, 100, 64, 0))]
    #[case::offset_out_of_bounds(build_packet_header(32, 100, 1000, 0))]
    #[case::huge_packet_count(build_packet_header(32, 100, 64, u64::MAX))]
    #[case::large_offset_overflow(build_packet_header(32, 100, u64::MAX, 0))]
    #[case::offset_causes_wrap(build_packet_header(32, 100, (usize::MAX - 16) as u64, 0))]
    #[case::all_zeros(vec![0u8; 128])]
    #[case::truncated_packets_array(build_packet_err_truncated_array())]
    #[case::partial_packet(build_packet_err_partial_packet())]
    #[case::one_byte_short_for_packet(build_packet_err_one_byte_short_for_packet())]
    #[case::all_ones(build_packet_err_all_ones())]
    #[case::packet_count_multiplication_overflow(build_packet_err_multiplication_overflow())]
    #[case::packets_length_at_boundary({
        let mut d = Vec::new();
        d.extend_from_slice(&encode_u256(32));
        d.extend_from_slice(&encode_u256(100));
        d.extend_from_slice(&encode_u256(64));
        d
    })]
    fn test_decode_packet_attestation_err(#[case] data: Vec<u8>) {
        assert!(decode_packet_attestation(&data).is_err());
    }

    // ==================== PacketAttestation success tests ====================

    #[test]
    fn test_decode_packet_attestation_empty_packets() {
        let data = build_packet_header(32, 100, 64, 0);
        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 100);
        assert!(result.packets.is_empty());
    }

    #[test]
    fn test_decode_packet_attestation_single_packet() {
        let path = [0xabu8; 32];
        let commitment = [0xcdu8; 32];

        let mut data = build_packet_header(32, 500, 64, 1);
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

        let mut data = build_packet_header(32, 999, 64, packets_data.len() as u64);
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

    #[rstest]
    #[case::zero_height(0)]
    #[case::max_height(u64::MAX)]
    fn test_decode_packet_attestation_height_boundary(#[case] height: u64) {
        let data = build_packet_header(32, height, 64, 0);
        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, height);
    }

    #[test]
    fn test_decode_packet_attestation_extra_data_ignored() {
        let mut data = build_packet_header(32, 42, 64, 0);
        data.extend_from_slice(&[0xffu8; 128]); // Extra garbage

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 42);
        assert!(result.packets.is_empty());
    }

    #[test]
    fn test_decode_packet_attestation_many_packets() {
        let packet_count = 100;
        let mut data = build_packet_header(32, 12345, 64, packet_count);

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

        let mut data = build_packet_header(32, 777, 64, 1);
        data.extend_from_slice(&path1);
        data.extend_from_slice(&commitment1);

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.packets[0].path[0..4], [0xde, 0xad, 0xbe, 0xef]);
        assert_eq!(
            result.packets[0].commitment[28..32],
            [0xca, 0xfe, 0xba, 0xbe]
        );
    }

    #[test]
    fn test_decode_packet_attestation_exactly_minimum_size() {
        let data = build_packet_header(32, 1, 64, 0);
        assert_eq!(data.len(), 128);

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 1);
        assert!(result.packets.is_empty());
    }

    #[test]
    fn test_decode_packet_attestation_non_zero_high_bytes_in_height() {
        let mut data = Vec::new();
        data.extend_from_slice(&encode_u256(32));

        let mut height_word = [0xffu8; 32];
        height_word[24..32].copy_from_slice(&999u64.to_be_bytes());
        data.extend_from_slice(&height_word);

        data.extend_from_slice(&encode_u256(64));
        data.extend_from_slice(&encode_u256(0));

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.height, 999);
    }

    #[test]
    fn test_decode_packet_attestation_valid_with_zero_path_and_commitment() {
        let mut data = build_packet_header(32, 100, 64, 1);
        data.extend_from_slice(&[0u8; 32]); // zero path
        data.extend_from_slice(&[0u8; 32]); // zero commitment

        let result = decode_packet_attestation(&data).unwrap();
        assert_eq!(result.packets.len(), 1);
        assert_eq!(result.packets[0].path, [0u8; 32]);
        assert_eq!(result.packets[0].commitment, [0u8; 32]);
    }
}
