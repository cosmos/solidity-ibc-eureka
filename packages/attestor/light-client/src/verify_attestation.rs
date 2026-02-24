//! Generic function and data structures for verifying
//! attested data.

use std::collections::HashSet;

use alloy_primitives::B256;
use ethereum_keys::recover::recover_address_from_prehash;
use sha2::{Digest, Sha256};

use crate::{client_state::ClientState, error::IbcAttestorClientError};

/// Distinguishes attestation types to prevent cross-protocol signature replay.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum AttestationType {
    /// State attestation (update client)
    State = 0x01,
    /// Packet attestation (membership/non-membership proofs)
    Packet = 0x02,
}

/// Length of the domain-separated signing preimage: 1-byte type tag + 32-byte SHA-256 hash.
const DOMAIN_SEPARATED_PREIMAGE_LEN: usize = 1 + 32;

/// Compute the prehash for signature verification with domain separation:
/// `sha256(type_tag || sha256(data))`
fn tagged_signing_input(data: &[u8], attestation_type: AttestationType) -> B256 {
    let inner_hash = Sha256::digest(data);
    let mut tagged = Vec::with_capacity(DOMAIN_SEPARATED_PREIMAGE_LEN);
    tagged.push(attestation_type as u8);
    tagged.extend_from_slice(&inner_hash);
    B256::from_slice(&Sha256::digest(&tagged))
}

/// Verifies the cryptographic validity of the attestation data using address recovery.
/// This function takes raw 65-byte signatures and recovers addresses to verify against the client state.
///
/// Fails if:
/// - Too few or duplicate signatures are provided
/// - A signature cannot recover to a valid address
/// - A recovered address is not in the client's trusted attestor set
#[allow(clippy::module_name_repetitions)]
pub(crate) fn verify_attestation(
    client_state: &ClientState,
    attestation_data: &[u8],
    raw_signatures: &[Vec<u8>],
    attestation_type: AttestationType,
) -> Result<(), IbcAttestorClientError> {
    if raw_signatures.is_empty() {
        return Err(IbcAttestorClientError::InvalidAttestedData {
            reason: "no signatures provided".into(),
        });
    }

    if raw_signatures.len() < client_state.min_required_sigs as usize {
        return Err(IbcAttestorClientError::InvalidAttestedData {
            reason: "too few signatures provided".into(),
        });
    }

    // Check for duplicate signatures
    let unique_sigs: HashSet<&Vec<u8>> = raw_signatures.iter().collect();
    if unique_sigs.len() != raw_signatures.len() {
        return Err(IbcAttestorClientError::InvalidAttestedData {
            reason: "duplicate signatures provided".into(),
        });
    }

    let prehash = tagged_signing_input(attestation_data, attestation_type);

    // Verify each signature by recovering its address
    for raw_sig in raw_signatures {
        let recovered_address = recover_address_from_prehash(&prehash, raw_sig)
            .map_err(|_| IbcAttestorClientError::InvalidSignature)?
            .into();

        // Check if the recovered address is in the trusted attestor set
        let is_trusted = client_state
            .attestor_addresses
            .iter()
            .any(|trusted_addr| trusted_addr == recovered_address);

        if !is_trusted {
            return Err(IbcAttestorClientError::UnknownAddressRecovered {
                address: recovered_address,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;

    fn create_test_signature_and_address(
        data: &[u8],
        attestation_type: AttestationType,
    ) -> (Vec<u8>, [u8; 20]) {
        let signer = PrivateKeySigner::random();
        let hash = tagged_signing_input(data, attestation_type);
        let signature = signer.sign_hash_sync(&hash).unwrap();
        let sig65 = signature.as_bytes().to_vec();
        let address = signer.address();

        (sig65, address.into())
    }

    #[test]
    fn test_verify_attestation_with_65_byte_signatures() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";

        // Create 3 test signatures from different keys
        let mut signatures = Vec::new();
        let mut addresses = Vec::new();

        for _ in 0..3 {
            let (sig, addr) =
                create_test_signature_and_address(attestation_data, AttestationType::State);
            signatures.push(sig);
            addresses.push(Address::from(addr));
        }

        // Create client state with the trusted addresses
        let client_state = ClientState {
            attestor_addresses: addresses.clone(),
            min_required_sigs: 2,
            latest_height: 0,
            is_frozen: false,
        };

        // Verify with enough signatures (should succeed)
        let result = verify_attestation(
            &client_state,
            attestation_data,
            &signatures[0..2],
            AttestationType::State,
        );
        assert!(result.is_ok());

        // Verify with all signatures (should succeed)
        let result = verify_attestation(
            &client_state,
            attestation_data,
            &signatures,
            AttestationType::State,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_attestation_fails_with_too_few_signatures() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";
        let (sig, addr) =
            create_test_signature_and_address(attestation_data, AttestationType::State);

        let client_state = ClientState {
            attestor_addresses: vec![Address::from(addr)],
            min_required_sigs: 2,
            latest_height: 0,
            is_frozen: false,
        };

        // Only 1 signature when 2 are required
        let result = verify_attestation(
            &client_state,
            attestation_data,
            &[sig],
            AttestationType::State,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IbcAttestorClientError::InvalidAttestedData { .. }
        ));
    }

    #[test]
    fn test_verify_attestation_fails_with_untrusted_signer() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";
        let (sig, _addr) =
            create_test_signature_and_address(attestation_data, AttestationType::State);

        // Different address that didn't sign
        let wrong_address = Address::from([0x42; 20]);

        let client_state = ClientState {
            attestor_addresses: vec![wrong_address],
            min_required_sigs: 1,
            latest_height: 0,
            is_frozen: false,
        };

        let result = verify_attestation(
            &client_state,
            attestation_data,
            &[sig],
            AttestationType::State,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IbcAttestorClientError::UnknownAddressRecovered { .. }
        ));
    }

    #[test]
    fn test_verify_attestation_fails_with_duplicate_signatures() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";
        let (sig, addr) =
            create_test_signature_and_address(attestation_data, AttestationType::State);

        let client_state = ClientState {
            attestor_addresses: vec![Address::from(addr)],
            min_required_sigs: 2,
            latest_height: 0,
            is_frozen: false,
        };

        // Same signature twice
        let result = verify_attestation(
            &client_state,
            attestation_data,
            &[sig.clone(), sig],
            AttestationType::State,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IbcAttestorClientError::InvalidAttestedData { .. }
        ));
    }

    #[test]
    fn test_verify_attestation_rejects_cross_domain_replay() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";

        // Sign as State
        let (sig, addr) =
            create_test_signature_and_address(attestation_data, AttestationType::State);

        let client_state = ClientState {
            attestor_addresses: vec![Address::from(addr)],
            min_required_sigs: 1,
            latest_height: 0,
            is_frozen: false,
        };

        // Verify as Packet — must fail (cross-domain replay)
        let result = verify_attestation(
            &client_state,
            attestation_data,
            &[sig],
            AttestationType::Packet,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IbcAttestorClientError::UnknownAddressRecovered { .. }
        ));
    }

    #[test]
    fn test_verify_attestation_rejects_packet_sig_as_state() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";

        // Sign as Packet
        let (sig, addr) =
            create_test_signature_and_address(attestation_data, AttestationType::Packet);

        let client_state = ClientState {
            attestor_addresses: vec![Address::from(addr)],
            min_required_sigs: 1,
            latest_height: 0,
            is_frozen: false,
        };

        // Verify as State — must fail
        let result = verify_attestation(
            &client_state,
            attestation_data,
            &[sig],
            AttestationType::State,
        );
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IbcAttestorClientError::UnknownAddressRecovered { .. }
        ));
    }

    #[test]
    fn test_verify_attestation_fails_with_empty_signatures() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";

        let client_state = ClientState {
            attestor_addresses: vec![Address::from([0x11; 20])],
            min_required_sigs: 1,
            latest_height: 0,
            is_frozen: false,
        };

        let result =
            verify_attestation(&client_state, attestation_data, &[], AttestationType::State);
        assert!(result.is_err());
        match result.unwrap_err() {
            IbcAttestorClientError::InvalidAttestedData { reason } => {
                assert_eq!(reason, "no signatures provided");
            }
            _ => panic!("expected InvalidAttestedData"),
        }
    }
}
