//! Membership proof verification for attestor client

use alloy_sol_types::SolValue;
use attestor_packet_membership::{verify_packet_membership, PacketCommitments};
use ibc_eureka_solidity_types::msgs::IAttestorMsgs;

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    verify_attestation,
};

/// Data structure that can be verified cryptographically
/// Matches the `AttestationProof` struct in IAttestorMsgs.sol
#[derive(Debug, Clone)]
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
    // Decode the ABI-encoded IAttestorMsgs::AttestationProof
    let attestation_proof = IAttestorMsgs::AttestationProof::abi_decode(&proof).map_err(|e| {
        IbcAttestorClientError::InvalidProof {
            reason: format!("Failed to decode ABI-encoded AttestationProof: {e}"),
        }
    })?;

    // Convert from Solidity type to our Rust type
    let attested_state = MembershipProof {
        attestation_data: attestation_proof.attestationData.to_vec(),
        signatures: attestation_proof
            .signatures
            .into_iter()
            .map(|sig| sig.to_vec())
            .collect(),
    };

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
    let packets =
        PacketCommitments::from_abi_bytes(&attested_state.attestation_data).map_err(|e| {
            IbcAttestorClientError::InvalidProof {
                reason: format!("Failed to decode ABI attestation data: {e}"),
            }
        })?;

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
        // Create the Solidity type and ABI encode it
        let attestation_proof = IAttestorMsgs::AttestationProof {
            attestationData: PACKET_COMMITMENTS_ENCODED.to_abi_bytes().into(),
            signatures: SIGS_RAW.clone().into_iter().map(|sig| sig.into()).collect(),
        };

        let as_bytes = attestation_proof.abi_encode();
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
        // Create the Solidity type and ABI encode it
        let attestation_proof = IAttestorMsgs::AttestationProof {
            attestationData: PACKET_COMMITMENTS_ENCODED.to_abi_bytes().into(),
            signatures: SIGS_RAW.clone().into_iter().map(|sig| sig.into()).collect(),
        };

        let as_bytes = attestation_proof.abi_encode();
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

        // Use a badly formed array that is valid JSON but invalid ABI
        let as_bytes = vec![0, 1, 3];
        let value = PACKET_COMMITMENTS[0].to_vec();
        let res = verify_membership(&cns, &cs, height, as_bytes, value);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::InvalidProof { .. })
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
        // Create the Solidity type and ABI encode it
        let attestation_proof = IAttestorMsgs::AttestationProof {
            attestationData: PACKET_COMMITMENTS_ENCODED.to_abi_bytes().into(),
            signatures: SIGS_RAW.clone().into_iter().map(|sig| sig.into()).collect(),
        };

        let as_bytes = attestation_proof.abi_encode();
        let value = PACKET_COMMITMENTS[0].to_vec();
        let res = verify_membership(&cns, &cs, height, as_bytes, value);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::UnknownAddressRecovered { .. })
        ));
    }
}
