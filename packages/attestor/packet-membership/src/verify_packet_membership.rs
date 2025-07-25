//! Generic function and data structures for verifying
//! the membership of IBC packets in a packet attestation.

use crate::PacketAttestationError;

/// Verifies that the provided `value` exists in the `proof`.
///
/// # Errors
/// - Individual packets cannot be deserialized
/// - The value cannot be deserialized
/// - The value does not exist in the proof
#[allow(clippy::module_name_repetitions, clippy::needless_pass_by_value)]
pub fn verify_packet_membership(
    proof: Vec<u8>,
    value: Vec<u8>,
) -> Result<(), PacketAttestationError> {
    let proof_packets: Vec<Vec<u8>> = serde_json::from_slice(&proof)
        .map_err(PacketAttestationError::SerdeDeserializationError)?;
    let value_packet: Vec<u8> = serde_json::from_slice(&value)
        .map_err(PacketAttestationError::SerdeDeserializationError)?;

    if proof_packets.iter().any(|packet| **packet == value_packet) {
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
    use super::*;

    #[test]
    fn succeeds() {
        let data = [b"cosmos rules", b"so does rust", b"hear, hear!!"];
        let packets: Vec<Vec<u8>> = data.into_iter().map(|d| d.to_vec()).collect();

        let proof = serde_json::to_vec(&packets).unwrap();
        let value = serde_json::to_vec(b"hear, hear!!".as_slice()).unwrap();

        let res = verify_packet_membership(proof, value);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_on_missing() {
        let data = [b"cosmos rules", b"so does rust", b"hear, hear!!"];

        let packets: Vec<Vec<u8>> = data.into_iter().map(|d| d.to_vec()).collect();

        let proof = serde_json::to_vec(&packets).unwrap();
        let value = serde_json::to_vec(b"this does not exist".as_slice()).unwrap();

        let res = verify_packet_membership(proof, value);
        assert!(
            matches!(res, Err(PacketAttestationError::VerificiationFailed { reason }) if reason.contains("not exist") )
        );
    }
}
