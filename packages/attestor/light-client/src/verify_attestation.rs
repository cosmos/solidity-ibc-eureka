//! Generic function and data structures for verifying
//! attested data.

use std::collections::HashSet;

use alloy_primitives::{Signature, B256};
use sha2::{Digest, Sha256};

use crate::{client_state::ClientState, error::IbcAttestorClientError};

/// Recover Ethereum address from 65-byte signature and message hash using alloy
/// # Errors
/// Returns an error if signature recovery fails or signature format is invalid
pub fn recover_address_from_signature(
    message_hash: &[u8; 32],
    signature_65: &[u8],
) -> Result<[u8; 20], IbcAttestorClientError> {
    // Parse the 65-byte signature using alloy-primitives
    let signature = Signature::try_from(signature_65)
        .map_err(|_| IbcAttestorClientError::InvalidSignature)?;

    // Convert message hash to B256
    let hash = B256::from_slice(message_hash);
    
    // Recover the address from the signature and pre-hashed message
    let address = signature.recover_address_from_prehash(&hash)
        .map_err(|_| IbcAttestorClientError::InvalidSignature)?;

    Ok(address.into())
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
) -> Result<(), IbcAttestorClientError> {
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

    // Hash the attestation data
    let mut hasher = Sha256::new();
    hasher.update(attestation_data);
    let message_hash = hasher.finalize();

    // Verify each signature by recovering its address
    for raw_sig in raw_signatures {
        let recovered_address = recover_address_from_signature(
            &message_hash.into(),
            raw_sig,
        )?;

        // Check if the recovered address is in the trusted attestor set
        let is_trusted = client_state
            .attestor_addresses
            .iter()
            .any(|trusted_addr| trusted_addr == recovered_address);

        if !is_trusted {
            return Err(IbcAttestorClientError::UnknownAddressRecovered { 
                address: recovered_address 
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
    
    fn create_test_signature_and_address(data: &[u8]) -> (Vec<u8>, [u8; 20]) {
        use sha2::{Digest, Sha256};
        
        let signer = PrivateKeySigner::random();
        
        let mut hasher = Sha256::new();
        hasher.update(data);
        let digest = hasher.finalize();
        let hash = B256::from_slice(&digest);
        
        let signature = signer.sign_hash_sync(&hash).unwrap();
        let sig65 = signature.as_bytes().to_vec();
        let address = signer.address();
        
        (sig65, address.into())
    }
    
    #[test]
    fn test_recover_address_from_65_byte_signature() {
        let message = b"test message";
        let mut hasher = Sha256::new();
        hasher.update(message);
        let message_hash: [u8; 32] = hasher.finalize().into();
        
        let (signature, expected_address) = create_test_signature_and_address(message);
        
        let recovered_address = recover_address_from_signature(&message_hash, &signature).unwrap();
        
        assert_eq!(recovered_address, expected_address);
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
        assert!(matches!(result.unwrap_err(), IbcAttestorClientError::InvalidAttestedData { .. }));
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
        assert!(matches!(result.unwrap_err(), IbcAttestorClientError::UnknownAddressRecovered { .. }));
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
        assert!(matches!(result.unwrap_err(), IbcAttestorClientError::InvalidAttestedData { .. }));
    }
}
