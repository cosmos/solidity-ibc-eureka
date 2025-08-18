//! Membership proof verification for attestor client

use serde::{Deserialize, Serialize};

use attestor_packet_membership::{verify_packet_membership, Packets};

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    verify_attestation,
};

/// Data structure that can be verified cryptographically
#[derive(Deserialize, Serialize)]
pub struct MembershipProof {
    /// Opaque abi-encoded data that was signed (abi.encode(packets))
    pub attestation_data: Vec<u8>,
    /// The original packets (decoded)
    pub packets: Packets,
    /// ECDSA signatures (64-byte r||s; 65-byte r||s||v accepted)
    pub signatures: Vec<Vec<u8>>,
    /// Compressed secp256k1 pubkeys (33 bytes) corresponding 1:1 with signatures
    pub public_keys: Vec<Vec<u8>>,
}

/// Verify membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
#[allow(clippy::needless_pass_by_value)]
pub fn verify_membership(
    consensus_state: &ConsensusState,
    client_state: &ClientState,
    height: u64,
    proof: Vec<u8>,
    value: Vec<u8>,
) -> Result<(), IbcAttestorClientError> {
    let attested_state: MembershipProof = serde_json::from_slice(&proof)
        .map_err(IbcAttestorClientError::DeserializeMembershipProofFailed)?;

    if consensus_state.height != height {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: "heights must match".into(),
        });
    }

    // attestation_data must equal abi.encode(packets)
    let encoded = abi_encode_packets(&attested_state.packets);
    if encoded != attested_state.attestation_data {
        return Err(IbcAttestorClientError::InvalidProof { reason: "attestation_data does not match abi.encode(packets)".into() });
    }

    verify_attestation::verify_attestation(
        client_state,
        &attested_state.attestation_data,
        &attested_state.signatures,
        &attested_state.public_keys,
    )?;

    verify_packet_membership::verify_packet_membership(attested_state.packets, value)?;

    Ok(())
}

pub(crate) fn abi_encode_packets(packets: &Packets) -> Vec<u8> {
    // Implements ABI encoding for a single value of type `bytes[]`.
    // Layout: length (32) | N offsets (32*N) | per-element (len (32) | data | padding)
    let lens: Vec<usize> = packets.packets().map(|p| p.len()).collect();
    let n = lens.len();
    // total head before first element data: 32 (length) + 32*n (offsets)
    let head_size = 32 + 32 * n;

    // Compute offsets into the tail region (relative to start after length)
    let mut offsets: Vec<usize> = Vec::with_capacity(n);
    let mut current = head_size;
    for li in &lens {
        offsets.push(current);
        let padded = ((li + 31) / 32) * 32;
        current += 32 + padded; // 32 for the element length, then padded data
    }

    let total_capacity = 32 + head_size + current; // approximate
    let mut out = Vec::with_capacity(total_capacity);

    // length (N)
    write_u256_be(&mut out, n as u128);

    // offsets
    for off in offsets {
        write_u256_be(&mut out, off as u128);
    }

    // per element: length, data, padding
    for (idx, data) in packets.packets().enumerate() {
        let li = lens[idx];
        write_u256_be(&mut out, li as u128);
        out.extend_from_slice(data);
        let padded = ((li + 31) / 32) * 32;
        let padding = padded - li;
        if padding > 0 {
            out.resize(out.len() + padding, 0u8);
        }
    }

    out
}

fn write_u256_be(buf: &mut Vec<u8>, val: u128) {
    let mut word = [0u8; 32];
    let be = val.to_be_bytes();
    // place the 16-byte be into the last 16 bytes of the 32-byte word
    word[16..32].copy_from_slice(&be);
    buf.extend_from_slice(&word);
}

/// Verify non-membership proof - only works for heights that exist in consensus state
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
pub fn verify_non_membership(
    _consensus_state: &ConsensusState,
    _client_state: &ClientState,
    _height: u64,
    _proof: Vec<u8>,
) -> Result<(), IbcAttestorClientError> {
    todo!()
}

#[cfg(test)]
mod verify_membership {
    use crate::test_utils::{PACKET_COMMITMENTS, PACKET_COMMITMENTS_ENCODED, PUBKEYS, SIGNERS, SIGS};

    use super::*;

    #[test]
    fn succeeds() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, min_required_sigs: 5, is_frozen: false };

        let height = cns.height;
        let attestation = MembershipProof { attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), packets: (*PACKET_COMMITMENTS_ENCODED).clone(), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = PACKET_COMMITMENTS[0].to_vec();
        let res = verify_membership(&cns, &cs, height, as_bytes, value);
        println!("{:?}", res);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_if_height_is_incorrect() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, min_required_sigs: 5, is_frozen: false };

        let bad_height = cns.height + 1;
        let attestation = MembershipProof { attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), packets: (*PACKET_COMMITMENTS_ENCODED).clone(), signatures: SIGS.clone(), public_keys: PUBKEYS.clone() };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = PACKET_COMMITMENTS[0].to_vec();
        let res = verify_membership(&cns, &cs, bad_height, as_bytes, value);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidProof { reason }) if reason.contains("height"))
        );
    }

    #[test]
    fn fails_if_proof_bad() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, min_required_sigs: 5, is_frozen: false };

        let height = cns.height;
        let attestation = [0, 1, 3].to_vec();

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = PACKET_COMMITMENTS[0].to_vec();
        let res = verify_membership(&cns, &cs, height, as_bytes, value);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::DeserializeMembershipProofFailed { .. })
        ));
    }

    // NOTE: We don't need to test every verification failure here
    // as this is extensively tested in the `verify` module
    #[test]
    fn fails_if_verification_fails() {
        let mut bad_pubkeys = PUBKEYS.clone();
        bad_pubkeys.pop();
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let cs = ClientState { attestors: SIGNERS.clone(), latest_height: 100, min_required_sigs: 5, is_frozen: false };

        let height = cns.height;
        let attestation = MembershipProof { attestation_data: crate::membership::abi_encode_packets(&PACKET_COMMITMENTS_ENCODED), packets: (*PACKET_COMMITMENTS_ENCODED).clone(), signatures: SIGS.clone(), public_keys: bad_pubkeys };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = serde_json::to_vec(PACKET_COMMITMENTS[0]).unwrap();
        let res = verify_membership(&cns, &cs, height, as_bytes, value);
        assert!(matches!(res, Err(IbcAttestorClientError::InvalidAttestedData { .. })));
    }
}
