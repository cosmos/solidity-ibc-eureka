//! Generic function and data structures for verifying
//! the membership of IBC packets in a packet attestation.
use borsh::from_slice;

use crate::PacketAttestationError;

/// Verifies that the provided `value` exists in the `proof`.
///
/// Fails if:
/// - Individual packets cannot be deserialized
/// - The value cannot be deserialized
/// - The value does not exist in the proof
#[allow(clippy::module_name_repetitions)]
pub fn verify_packet_membership(
    proof: Vec<u8>,
    value: Vec<u8>,
) -> Result<(), PacketAttestationError> {
    let proof_packets: Vec<Vec<u8>> =
        from_slice(&proof).map_err(|e| PacketAttestationError::BorshDeserializationError(e))?;
    let value_packet: Vec<u8> =
        from_slice(&value).map_err(|e| PacketAttestationError::BorshDeserializationError(e))?;

    if proof_packets
        .iter()
        .map(|packet| packet)
        .find(|packet| **packet == value_packet)
        .is_some()
    {
        Ok(())
    } else {
        Err(PacketAttestationError::VerificiationFailed {
            reason: "value does not exist in proof".into(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::module_name_repetitions)]
mod verify_packet_membership {
    use borsh::to_vec;
    use ibc::core::channel::types::{
        commitment::{compute_packet_commitment, PacketCommitment},
        timeout::{TimeoutHeight, TimeoutTimestamp},
    };

    use super::*;

    #[test]
    fn succeeds() {
        let data = [b"cosmos rules", b"so does rust", b"hear, hear!!"];

        let timeout_height = TimeoutHeight::Never;
        let timeout_timestamp = TimeoutTimestamp::Never;

        let packets: Vec<PacketCommitment> = data
            .into_iter()
            .map(|d| compute_packet_commitment(d, &timeout_height, &timeout_timestamp))
            .collect();

        let proof = to_vec(&packets).unwrap();

        let value = to_vec(&compute_packet_commitment(
            b"hear, hear!!".as_slice(),
            &timeout_height,
            &timeout_timestamp,
        ))
        .unwrap();

        let res = verify_packet_membership(proof, value);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_on_missing() {
        let data = [b"cosmos rules", b"so does rust", b"hear, hear!!"];

        let timeout_height = TimeoutHeight::Never;
        let timeout_timestamp = TimeoutTimestamp::Never;

        let packets: Vec<PacketCommitment> = data
            .into_iter()
            .map(|d| compute_packet_commitment(d, &timeout_height, &timeout_timestamp))
            .collect();

        let proof = to_vec(&packets).unwrap();

        let value = to_vec(&compute_packet_commitment(
            b"this does not exist".as_slice(),
            &timeout_height,
            &timeout_timestamp,
        ))
        .unwrap();

        let res = verify_packet_membership(proof, value);
        assert!(
            matches!(res, Err(PacketAttestationError::VerificiationFailed { reason }) if reason.contains("not exist") )
        );
    }
}
