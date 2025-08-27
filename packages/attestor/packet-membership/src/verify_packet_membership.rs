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
        .packets()
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
    use alloy_primitives::FixedBytes;

    #[test]
    fn succeeds() {
        let data: Vec<[u8; 32]> = vec![[7u8; 32], [8u8; 32], [9u8; 32]];
        let packets: Vec<FixedBytes<32>> = data.iter().map(|d| (*d).into()).collect();

        let proof = PacketCommitments::new(packets);
        let value = [9u8; 32].to_vec();

        let res = verify_packet_membership(proof, value);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_on_missing() {
        let data: Vec<[u8; 32]> = vec![[7u8; 32], [8u8; 32], [9u8; 32]];
        let packets: Vec<FixedBytes<32>> = data.iter().map(|d| (*d).into()).collect();

        let proof = PacketCommitments::new(packets);
        let value = [0u8; 32].to_vec();

        let res = verify_packet_membership(proof, value);
        assert!(
            matches!(res, Err(PacketAttestationError::VerificiationFailed { reason }) if reason.contains("not exist"))
        );
    }
}
