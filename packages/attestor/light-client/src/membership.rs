//! Membership proof verification for attestor client

use alloy_primitives::hex;
use alloy_sol_types::SolType;
use ibc_eureka_solidity_types::msgs::IAttestationMsgs;
use serde::{Deserialize, Serialize};

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::IbcAttestorClientError,
    verify_attestation,
};

/// Data structure that can be verified cryptographically
/// Matches the `AttestationProof` struct in IAttestationMsgs.sol
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
    proof: Vec<u8>,
    path: Vec<Vec<u8>>,
    value: Vec<u8>,
) -> Result<(), IbcAttestorClientError> {
    if path.len() != 1 {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: format!("Expected path length 1, got {}", path.len()),
        });
    }

    let attested_state: MembershipProof = serde_json::from_slice(&proof)
        .map_err(IbcAttestorClientError::DeserializeMembershipProofFailed)?;

    let proof_data = IAttestationMsgs::PacketAttestation::abi_decode(
        &attested_state.attestation_data,
    )
    .map_err(|e| IbcAttestorClientError::InvalidProof {
        reason: format!("Failed to decode ABI attestation data: {e}"),
    })?;

    let trusted_height = consensus_state.height;
    if trusted_height != proof_data.height {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: "trusted consensus and proof height must match".into(),
        });
    }

    verify_attestation::verify_attestation(
        client_state,
        &attested_state.attestation_data,
        &attested_state.signatures,
        verify_attestation::AttestationType::Packet,
    )?;

    if proof_data.packets.is_empty() {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: "Membership proof failed: no packets in attestation".into(),
        });
    }

    let path_hash: [u8; 32] = alloy_primitives::keccak256(&path[0]).into();

    let packet = proof_data
        .packets
        .iter()
        .find(|p| p.path == path_hash)
        .ok_or_else(|| IbcAttestorClientError::InvalidProof {
            reason: format!(
                "Membership proof failed: path 0x{} not found in attested packets",
                hex::encode(path_hash)
            ),
        })?;

    if packet.commitment.as_slice() != value.as_slice() {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: format!(
                "Membership proof failed: commitment mismatch for path 0x{}",
                hex::encode(path_hash)
            ),
        });
    }

    Ok(())
}

/// Verify non-membership proof - only works for heights that exist in consensus state
/// For non-membership (timeout proofs), we verify that the specific path has a ZERO commitment
/// # Errors
/// Returns an error if the height is not found in consensus state or proof verification fails
#[allow(clippy::needless_pass_by_value)]
pub fn verify_non_membership(
    consensus_state: &ConsensusState,
    client_state: &ClientState,
    proof: Vec<u8>,
    path: Vec<Vec<u8>>,
) -> Result<(), IbcAttestorClientError> {
    if path.len() != 1 {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: format!("Expected path length 1, got {}", path.len()),
        });
    }

    let attested_state: MembershipProof = serde_json::from_slice(&proof)
        .map_err(IbcAttestorClientError::DeserializeMembershipProofFailed)?;

    let proof_data = IAttestationMsgs::PacketAttestation::abi_decode(
        &attested_state.attestation_data,
    )
    .map_err(|e| IbcAttestorClientError::InvalidProof {
        reason: format!("Failed to decode ABI attestation data: {e}"),
    })?;

    let trusted_height = consensus_state.height;
    if trusted_height != proof_data.height {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: "trusted consensus and proof height must match".into(),
        });
    }

    verify_attestation::verify_attestation(
        client_state,
        &attested_state.attestation_data,
        &attested_state.signatures,
        verify_attestation::AttestationType::Packet,
    )?;

    if proof_data.packets.is_empty() {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: "Non-membership proof failed: no packets in attestation".into(),
        });
    }

    let path_hash: [u8; 32] = alloy_primitives::keccak256(&path[0]).into();

    let packet = proof_data
        .packets
        .iter()
        .find(|p| p.path == path_hash)
        .ok_or_else(|| IbcAttestorClientError::InvalidProof {
            reason: format!(
                "Non-membership proof failed: path 0x{} not found in attested packets",
                hex::encode(path_hash)
            ),
        })?;

    if packet.commitment != [0u8; 32] {
        return Err(IbcAttestorClientError::InvalidProof {
            reason: format!(
                "Non-membership proof failed: commitment for path 0x{} is not zero",
                hex::encode(packet.path)
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod verify_membership_tests {
    use alloy_sol_types::SolValue;

    use crate::test_utils::{
        packet_commitments_with_height, sample_packet_commitments, sigs_with_height, ADDRESSES,
        MEMBERSHIP_PATH,
    };

    use super::*;

    fn default_packet_commitments() -> IAttestationMsgs::PacketAttestation {
        packet_commitments_with_height(100)
    }
    fn default_sigs() -> Vec<Vec<u8>> {
        sigs_with_height(100)
    }
    fn default_path() -> Vec<Vec<u8>> {
        vec![MEMBERSHIP_PATH.to_vec()]
    }

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

        let attestation = MembershipProof {
            attestation_data: default_packet_commitments().abi_encode(),
            signatures: default_sigs(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let path = default_path();
        let value = sample_packet_commitments()[0].commitment.to_vec();
        let res = verify_membership(&cns, &cs, as_bytes, path, value);
        println!("{res:?}");
        assert!(res.is_ok());
    }

    #[test]
    fn fails_if_path_empty() {
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

        let attestation = MembershipProof {
            attestation_data: default_packet_commitments().abi_encode(),
            signatures: default_sigs(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let path: Vec<Vec<u8>> = vec![];
        let value = sample_packet_commitments()[0].commitment.to_vec();
        let res = verify_membership(&cns, &cs, as_bytes, path, value);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidProof { reason }) if reason.contains("Expected path length 1"))
        );
    }

    #[test]
    fn fails_if_path_length_greater_than_one() {
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

        let attestation = MembershipProof {
            attestation_data: default_packet_commitments().abi_encode(),
            signatures: default_sigs(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let path: Vec<Vec<u8>> = vec![b"ibc".to_vec(), MEMBERSHIP_PATH.to_vec()];
        let value = sample_packet_commitments()[0].commitment.to_vec();
        let res = verify_membership(&cns, &cs, as_bytes, path, value);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidProof { reason }) if reason.contains("Expected path length 1"))
        );
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
        let packets_with_bad_height = packet_commitments_with_height(bad_height);
        let attestation = MembershipProof {
            attestation_data: packets_with_bad_height.abi_encode(),
            signatures: default_sigs(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let path = default_path();
        let value = sample_packet_commitments()[0].commitment.to_vec();
        let res = verify_membership(&cns, &cs, as_bytes, path, value);
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

        let attestation = [0, 1, 3].to_vec();

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let path = default_path();
        let value = sample_packet_commitments()[0].commitment.to_vec();
        let res = verify_membership(&cns, &cs, as_bytes, path, value);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::DeserializeMembershipProofFailed { .. })
        ));
    }

    #[test]
    fn fails_if_verification_fails() {
        let cns = ConsensusState {
            height: 100,
            timestamp: 123,
        };
        let cs = ClientState {
            attestor_addresses: Vec::new(),
            latest_height: 100,
            min_required_sigs: 5,
            is_frozen: false,
        };

        let attestation = MembershipProof {
            attestation_data: default_packet_commitments().abi_encode(),
            signatures: default_sigs(),
        };

        let as_bytes = serde_json::to_vec(&attestation).unwrap();
        let path = default_path();
        let value = sample_packet_commitments()[0].commitment.to_vec();
        let res = verify_membership(&cns, &cs, as_bytes, path, value);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::UnknownAddressRecovered { .. })
        ));
    }
}

#[cfg(test)]
mod verify_non_membership_tests {
    use alloy_sol_types::SolValue;

    use crate::test_utils::{
        packet_commitments_with_height, sigs_with_height, ADDRESSES, NON_MEMBERSHIP_PATH,
    };

    use super::*;

    #[test]
    fn succeeds_when_path_has_zero_commitment() {
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

        let attestation = MembershipProof {
            attestation_data: packet_commitments_with_height(100).abi_encode(),
            signatures: sigs_with_height(100),
        };

        let proof_bytes = serde_json::to_vec(&attestation).unwrap();
        let path = vec![NON_MEMBERSHIP_PATH.to_vec()];

        let res = verify_non_membership(&cns, &cs, proof_bytes, path);
        assert!(res.is_ok());
    }

    #[test]
    fn fails_if_path_empty() {
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

        let attestation = MembershipProof {
            attestation_data: packet_commitments_with_height(100).abi_encode(),
            signatures: sigs_with_height(100),
        };

        let proof_bytes = serde_json::to_vec(&attestation).unwrap();
        let path: Vec<Vec<u8>> = vec![];

        let res = verify_non_membership(&cns, &cs, proof_bytes, path);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidProof { reason }) if reason.contains("Expected path length 1"))
        );
    }

    #[test]
    fn fails_if_path_length_greater_than_one() {
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

        let attestation = MembershipProof {
            attestation_data: packet_commitments_with_height(100).abi_encode(),
            signatures: sigs_with_height(100),
        };

        let proof_bytes = serde_json::to_vec(&attestation).unwrap();
        let path: Vec<Vec<u8>> = vec![b"ibc".to_vec(), NON_MEMBERSHIP_PATH.to_vec()];

        let res = verify_non_membership(&cns, &cs, proof_bytes, path);
        assert!(
            matches!(res, Err(IbcAttestorClientError::InvalidProof { reason }) if reason.contains("Expected path length 1"))
        );
    }

    #[test]
    fn fails_when_path_not_in_attestation() {
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

        let attestation = MembershipProof {
            attestation_data: packet_commitments_with_height(100).abi_encode(),
            signatures: sigs_with_height(100),
        };

        let proof_bytes = serde_json::to_vec(&attestation).unwrap();
        let path = vec![b"unknown-path".to_vec()];

        let res = verify_non_membership(&cns, &cs, proof_bytes, path);
        assert!(matches!(
            res,
            Err(IbcAttestorClientError::InvalidProof { reason }) if reason.contains("not found")
        ));
    }
}
