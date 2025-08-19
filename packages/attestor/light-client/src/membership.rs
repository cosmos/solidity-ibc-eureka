//! Membership proof verification for attestor client

use k256::ecdsa::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};

use attestor_packet_membership::{verify_packet_membership, Packets};

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    verify_attestation,
};

/// Data structure that can be verified cryptographically
#[derive(Deserialize, Serialize)]
pub struct MembershipProof {
    /// Opaque serde-encoded data that was signed
    pub attestation_data: Packets,
    /// Signatures of the attestors
    pub signatures: Vec<Signature>,
    /// Public keys of the attestors submitting attestations
    pub pubkeys: Vec<VerifyingKey>,
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

    let hashable_bytes: Vec<u8> = serde_json::to_vec(&attested_state.attestation_data)
        .map_err(IbcAttestorClientError::DeserializeMembershipProofFailed)?;
    verify_attestation::verify_attestation(
        client_state,
        &hashable_bytes,
        &attested_state.signatures,
        &attested_state.pubkeys,
    )?;

    verify_packet_membership::verify_packet_membership(attested_state.attestation_data, value)?;

    Ok(())
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
    use crate::test_utils::{KEYS, PACKET_COMMITMENTS, PACKET_COMMITMENTS_ENCODED, SIGS};

    use super::*;

    #[test]
    fn succeeds() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let height = cns.height;
        let attestation = MembershipProof {
            attestation_data: PACKET_COMMITMENTS_ENCODED.clone(),
            pubkeys: KEYS.clone(),
            signatures: SIGS.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = PACKET_COMMITMENTS[0].clone();
        let res = verify_membership(&cns, &cs, height, as_bytes, value.to_vec());
        println!("{:?}", res);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_if_height_is_incorrect() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let bad_height = cns.height + 1;
        let attestation = MembershipProof {
            attestation_data: (*PACKET_COMMITMENTS_ENCODED).clone(),
            pubkeys: KEYS.clone(),
            signatures: SIGS.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = serde_json::to_vec(PACKET_COMMITMENTS[0]).unwrap();
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
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let height = cns.height;
        let attestation = [0, 1, 3].to_vec();

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = serde_json::to_vec(PACKET_COMMITMENTS[0]).unwrap();
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
        let mut bad_keys = KEYS.clone();
        bad_keys.pop();
        let cns = ConsensusState {
            height: 100,
            timestamp: 123456789,
        };
        let cs = ClientState {
            pub_keys: KEYS.clone(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let height = cns.height;
        let attestation = MembershipProof {
            attestation_data: PACKET_COMMITMENTS_ENCODED.clone(),
            pubkeys: bad_keys,
            signatures: SIGS.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = serde_json::to_vec(PACKET_COMMITMENTS[0]).unwrap();
        let res = verify_membership(&cns, &cs, height, as_bytes, value);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::InvalidAttestedData { reason }) if reason.contains("keys")
        ));
    }
}
