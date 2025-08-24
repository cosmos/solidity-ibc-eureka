//! Generic function and data structures for verifying
//! the membership of IBC packets in a packet attestation.

use crate::PacketAttestationError;
use crate::Packets;

/// Verifies that the provided `value` exists in the `proof`.
///
/// # Errors
/// - The value does not exist in the proof
#[allow(clippy::module_name_repetitions, clippy::needless_pass_by_value)]
pub fn verify_packet_membership(
    proof: Packets,
    value: Vec<u8>,
) -> Result<(), PacketAttestationError> {
    if proof.packets().any(|packet| *packet == value) {
        Ok(())
    } else {
        Err(PacketAttestationError::VerificiationFailed {
            reason: "value does not exist in proof".into(),
        })
    }
}

#[cfg(test)]
#[allow(clippy::module_inception)]
mod verify_packet_membership {
    use super::*;

    #[test]
    fn succeeds() {
        let data = [b"cosmos rules", b"so does rust", b"hear, hear!!"];
        let packets: Vec<Vec<u8>> = data.into_iter().map(|d| d.to_vec()).collect();

        let proof = Packets::new(packets);
        let value = b"hear, hear!!".to_vec();

        let res = verify_packet_membership(proof, value);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_on_missing() {
        let data = [b"cosmos rules", b"so does rust", b"hear, hear!!"];

        let packets: Vec<Vec<u8>> = data.into_iter().map(|d| d.to_vec()).collect();

        let proof = Packets::new(packets);
        let value = b"this does not exist".to_vec();

        let res = verify_packet_membership(proof, value);
        assert!(
            matches!(res, Err(PacketAttestationError::VerificiationFailed { reason }) if reason.contains("not exist"))
        );
    }
}
