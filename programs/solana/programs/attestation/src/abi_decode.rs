use crate::error::ErrorCode;
use crate::types::{PacketAttestation, PacketCommitment, StateAttestation};
use alloy_sol_types::SolValue;
use anchor_lang::prelude::*;

mod sol_types {
    alloy_sol_types::sol!(
        "../../../../contracts/light-clients/attestation/msgs/IAttestationMsgs.sol"
    );
}

use sol_types::IAttestationMsgs;

/// Decode a `PacketAttestation` from ABI-encoded bytes.
pub fn decode_packet_attestation(data: &[u8]) -> Result<PacketAttestation> {
    let decoded = IAttestationMsgs::PacketAttestation::abi_decode(data)
        .map_err(|_| error!(ErrorCode::InvalidAttestationData))?;

    let packets = decoded
        .packets
        .into_iter()
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

/// Decode a `StateAttestation` from ABI-encoded bytes.
pub fn decode_state_attestation(data: &[u8]) -> Result<StateAttestation> {
    let decoded = IAttestationMsgs::StateAttestation::abi_decode(data)
        .map_err(|_| error!(ErrorCode::InvalidAttestationData))?;

    Ok(StateAttestation {
        height: decoded.height,
        timestamp: decoded.timestamp,
    })
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

    /// Build a standard packet attestation header.
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
        // Lenient decoder truncates u256 to u64, ignoring high bytes
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
    #[case::empty(vec![])]
    #[case::invalid_tuple_offset_wrong(build_packet_header(64, 100, 64, 0))]
    #[case::offset_out_of_bounds(build_packet_header(32, 100, 1000, 0))]
    #[case::huge_packet_count(build_packet_header(32, 100, 64, u64::MAX))]
    #[case::large_offset_overflow(build_packet_header(32, 100, u64::MAX, 0))]
    #[case::offset_causes_wrap(build_packet_header(32, 100, (usize::MAX - 16) as u64, 0))]
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
        // Lenient decoder truncates u256 to u64, ignoring high bytes
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
