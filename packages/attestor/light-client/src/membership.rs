//! Membership proof verification for attestor client

use secp256k1::{ecdsa::Signature, PublicKey};
use serde::Deserialize;

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    verify_attestation,
};

/// Data structure that can be verified cryptographically
#[cfg_attr(test, derive(serde::Serialize))]
#[derive(Deserialize)]
pub struct Verifyable {
    /// Opaque borsh-encoded data that was signed
    attestation_data: Vec<u8>,
    /// Signatures of the attestors
    signatures: Vec<Signature>,
    /// Public keys of the attestors submitting attestations
    pubkeys: Vec<PublicKey>,
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
) -> Result<(), IbcAttestorClientError> {
    let attested_state: Verifyable = serde_json::from_slice(&proof)
        .map_err(IbcAttestorClientError::DeserializeMembershipProofFailed)?;

    if consensus_state.height != height {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: "heights must match".into(),
        });
    }

    verify_attestation::verify_attestation(
        client_state,
        &attested_state.attestation_data,
        &attested_state.signatures,
        &attested_state.pubkeys,
    )?;

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
    use crate::test_utils::{DUMMY_DATA, KEYS, SIGS};

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
        let attestation = Verifyable {
            attestation_data: DUMMY_DATA.to_vec(),
            pubkeys: KEYS.clone(),
            signatures: SIGS.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let res = verify_membership(&cns, &cs, height, as_bytes);
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
        let attestation = Verifyable {
            attestation_data: DUMMY_DATA.to_vec(),
            pubkeys: KEYS.clone(),
            signatures: SIGS.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let res = verify_membership(&cns, &cs, bad_height, as_bytes);
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
        let res = verify_membership(&cns, &cs, height, as_bytes);
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
        let attestation = Verifyable {
            attestation_data: DUMMY_DATA.to_vec(),
            pubkeys: bad_keys,
            signatures: SIGS.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let res = verify_membership(&cns, &cs, height, as_bytes);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::InvalidAttestedData { reason }) if reason.contains("keys")
        ));
    }
}
