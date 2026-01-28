//! Generic function and data structures for verifying
//! attested data.

use std::collections::HashSet;

use ethereum_keys::recover::recover_address;

use crate::{client_state::ClientState, error::IbcAttestorClientError};

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

    // Verify each signature by recovering its address
    for raw_sig in raw_signatures {
        let recovered_address = recover_address(attestation_data, raw_sig)
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

#[cfg_attr(coverage_nightly, coverage(off))]
#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::B256;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use sha2::Digest;

    fn create_test_signature_and_address(data: &[u8]) -> (Vec<u8>, [u8; 20]) {
        let signer = PrivateKeySigner::random();
        let hash = B256::from_slice(&sha2::Sha256::digest(data));
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
            let (sig, addr) = create_test_signature_and_address(attestation_data);
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
        let result = verify_attestation(&client_state, attestation_data, &signatures[0..2]);
        assert!(result.is_ok());

        // Verify with all signatures (should succeed)
        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_attestation_fails_with_too_few_signatures() {
        use alloy_primitives::Address;

        let attestation_data = b"test attestation data";
        let (sig, addr) = create_test_signature_and_address(attestation_data);

        let client_state = ClientState {
            attestor_addresses: vec![Address::from(addr)],
            min_required_sigs: 2,
            latest_height: 0,
            is_frozen: false,
        };

        // Only 1 signature when 2 are required
        let result = verify_attestation(&client_state, attestation_data, &[sig]);
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
        let (sig, _addr) = create_test_signature_and_address(attestation_data);

        // Different address that didn't sign
        let wrong_address = Address::from([0x42; 20]);

        let client_state = ClientState {
            attestor_addresses: vec![wrong_address],
            min_required_sigs: 1,
            latest_height: 0,
            is_frozen: false,
        };

        let result = verify_attestation(&client_state, attestation_data, &[sig]);
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
        let (sig, addr) = create_test_signature_and_address(attestation_data);

        let client_state = ClientState {
            attestor_addresses: vec![Address::from(addr)],
            min_required_sigs: 2,
            latest_height: 0,
            is_frozen: false,
        };

        // Same signature twice
        let result = verify_attestation(&client_state, attestation_data, &[sig.clone(), sig]);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            IbcAttestorClientError::InvalidAttestedData { .. }
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

        let result = verify_attestation(&client_state, attestation_data, &[]);
        assert!(result.is_err());
        match result.unwrap_err() {
            IbcAttestorClientError::InvalidAttestedData { reason } => {
                assert_eq!(reason, "no signatures provided");
            }
            _ => panic!("expected InvalidAttestedData"),
        }
    }
}
