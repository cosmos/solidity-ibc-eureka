//! Generic function and data structures for verifying
//! the membership of IBC packets in a packet attestation.

use crate::{packet_commitments::PacketCommitments, PacketAttestationError};

/// Verifies that the provided `value` exists in the `proof`.
///
/// # Errors
/// - The value does not exist in the proof
#[allow(clippy::module_name_repetitions, clippy::needless_pass_by_value)]
pub fn verify_packet_membership(
    proof: PacketCommitments,
    value: Vec<u8>,
) -> Result<(), PacketAttestationError> {
    if proof
        .commitments()
        .any(|packet| packet.as_slice() == value.as_slice())
    {
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

    use crate::packet_commitments::PacketCompact;

    #[test]
    fn succeeds() {
        // (path, commitment)[]
        let proof = PacketCommitments::new(vec![
            PacketCompact::new([1u8; 32], [2u8; 32]),
            PacketCompact::new([3u8; 32], [4u8; 32]),
        ]);

        let value = [4u8; 32].to_vec();

        let res = verify_packet_membership(proof, value);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_on_missing() {
         // (path, commitment)[]
         let proof = PacketCommitments::new(vec![
            PacketCompact::new([1u8; 32], [2u8; 32]),
            PacketCompact::new([3u8; 32], [4u8; 32]),
        ]);

        // commitment that is not in the proof
        let value = [7u8; 32].to_vec();

        let res = verify_packet_membership(proof, value);
        assert!(
            matches!(res, Err(PacketAttestationError::VerificiationFailed { reason }) if reason.contains("not exist"))
        );
    }
}
