//! Membership proof verification for attestor client

use alloy_sol_types::SolType;
use ibc_eureka_solidity_types::msgs::IAttestorMsgs;
use serde::{Deserialize, Serialize};

use attestor_packet_membership::{verify_packet_membership, PacketCommitments};

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    verify_attestation,
};

/// Data structure that can be verified cryptographically
/// Matches the `AttestationProof` struct in IAttestorMsgs.sol
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct MembershipProof {
    /// ABI-encoded bytes32[] of packet commitments (the actual attested data)
    pub attestation_data: Vec<u8>,
    /// Signatures over `sha256(attestation_data)`; each 65-byte (r||s||v)
    /// We recover addresses from these signatures instead of sending in public keys
    pub signatures: Vec<Vec<u8>>,
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

    // First verify the attestation signatures against the client state
    verify_attestation::verify_attestation(
        client_state,
        &attested_state.attestation_data,
        &attested_state.signatures,
    )?;

    // Decode the ABI-encoded attestation data to get the packet commitments
    let proof = IAttestorMsgs::PacketAttestation::abi_decode(&attested_state.attestation_data)
        .map_err(|e| IbcAttestorClientError::InvalidProof {
            reason: format!("Failed to decode ABI attestation data: {e}"),
        })?;

    let packets = PacketCommitments::new(proof.packetCommitments);
    verify_packet_membership::verify_packet_membership(packets, value)?;

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
    use alloy_sol_types::SolValue;

    use crate::test_utils::{ADDRESSES, PACKET_COMMITMENTS, PACKET_COMMITMENTS_ENCODED, SIGS_RAW};

    use super::*;

    #[test]
    fn succeeds() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let height = cns.height;
        let attestation = MembershipProof {
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = PACKET_COMMITMENTS[0];
        let res = verify_membership(&cns, &cs, height, as_bytes, value.to_vec());
        println!("{res:?}");
        assert!(res.is_ok());
    }

    #[test]
    fn fails_if_height_is_incorrect() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let bad_height = cns.height + 1;
        let attestation = MembershipProof {
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

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
            timestamp: 123,
        };
        let cs = ClientState {
            attestor_addresses: ADDRESSES.clone(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

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
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };
        // Empty attestor set will cause UnknownAddressRecovered
        let cs = ClientState {
            attestor_addresses: Vec::new(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let height = cns.height;
        let attestation = MembershipProof {
            attestation_data: PACKET_COMMITMENTS_ENCODED.abi_encode(),
            signatures: SIGS_RAW.clone(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let value = PACKET_COMMITMENTS[0].to_vec();
        let res = verify_membership(&cns, &cs, height, as_bytes, value);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::UnknownAddressRecovered { .. })
        ));
    }
}
