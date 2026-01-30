//! Attestation verification for the attestation light client.
//!
//! This module handles signature verification against a set of trusted attestors.

use anchor_lang::prelude::*;

use crate::crypto::recover_eth_address;
use crate::error::ErrorCode;
use crate::types::ClientState;

/// Verify attestation signatures and check recovered addresses against trusted set.
///
/// Validates that:
/// - At least one signature is provided
/// - Minimum required signatures threshold is met
/// - No duplicate recovered addresses (same signer cannot sign twice)
/// - All recovered addresses belong to trusted attestors
pub fn verify_attestation(
    client_state: &ClientState,
    attestation_data: &[u8],
    raw_signatures: &[Vec<u8>],
) -> Result<()> {
    if raw_signatures.is_empty() {
        return Err(error!(ErrorCode::EmptySignatures));
    }

    if raw_signatures.len() < client_state.min_required_sigs as usize {
        return Err(error!(ErrorCode::ThresholdNotMet));
    }

    // Recover addresses and check for duplicates + trust in single pass
    // Matches Solidity which checks duplicate recovered addresses, not signature bytes
    let mut recovered_addresses: Vec<[u8; 20]> = Vec::with_capacity(raw_signatures.len());

    for raw_sig in raw_signatures {
        let recovered_address = recover_eth_address(attestation_data, raw_sig)?;

        if recovered_addresses.contains(&recovered_address) {
            return Err(error!(ErrorCode::DuplicateSigner));
        }

        if !client_state.attestor_addresses.contains(&recovered_address) {
            return Err(error!(ErrorCode::UnknownSigner));
        }

        recovered_addresses.push(recovered_address);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    //! Unit tests for attestation verification logic.
    //!
    //! These tests run natively with `cargo test` (not in SVM). Tests using `TestAttestor`
    //! require actual signature recovery, which is provided by k256 crate in native builds
    //! (see `crypto.rs`). This enables fast iteration and debugging of verification logic.
    //!
    //! For integration tests that use the real Solana `secp256k1_recover` syscall,
    //! see the Mollusk-based tests in `instructions/*.rs`.

    use super::*;
    use crate::test_helpers::signing::TestAttestor;
    use crate::types::AccountVersion;

    fn create_test_client_state(
        attestor_addresses: Vec<[u8; 20]>,
        min_required_sigs: u8,
    ) -> ClientState {
        ClientState {
            version: AccountVersion::V1,
            client_id: "test-client".to_string(),
            attestor_addresses,
            min_required_sigs,
            latest_height: 100,
            is_frozen: false,
        }
    }

    fn create_test_signature(index: u8) -> Vec<u8> {
        let mut sig = vec![index; 64];
        sig.push(27); // recovery_id
        sig
    }

    #[test]
    fn test_verify_attestation_no_signatures() {
        let client_state = create_test_client_state(vec![[1u8; 20]], 1);
        let attestation_data = b"test data";
        let signatures: Vec<Vec<u8>> = vec![];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_too_few_signatures() {
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20]], 2);
        let attestation_data = b"test data";
        let signatures = vec![create_test_signature(1)];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_duplicate_signers() {
        // Duplicate signatures recover to same address, triggering DuplicateSigner error
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20]], 2);
        let attestation_data = b"test data";
        let sig = create_test_signature(1);
        let signatures = vec![sig.clone(), sig];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_min_sigs_zero_with_no_sigs() {
        let client_state = create_test_client_state(vec![[1u8; 20]], 0);
        let attestation_data = b"test data";
        let signatures: Vec<Vec<u8>> = vec![];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_different_signatures() {
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20]], 2);
        let attestation_data = b"test data";
        let sig1 = create_test_signature(1);
        let sig2 = create_test_signature(2);

        let result = verify_attestation(&client_state, attestation_data, &[sig1, sig2]);
        // Will fail at signature recovery (stub returns error), but not at duplicate check
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_exact_required_sigs() {
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 2);
        let attestation_data = b"test data";
        let signatures = vec![create_test_signature(1), create_test_signature(2)];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        // Will fail at signature recovery, but passes the count check
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_more_than_required_sigs() {
        let client_state = create_test_client_state(vec![[1u8; 20], [2u8; 20], [3u8; 20]], 2);
        let attestation_data = b"test data";
        let signatures = vec![
            create_test_signature(1),
            create_test_signature(2),
            create_test_signature(3),
        ];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        // Will fail at signature recovery, but passes the count check
        assert!(result.is_err());
    }

    // Tests below use TestAttestor which creates real ECDSA signatures.
    // Signature recovery uses k256 crate in native builds (see crypto.rs).

    #[test]
    fn test_verify_attestation_happy_path() {
        let attestor1 = TestAttestor::new(1);
        let attestor2 = TestAttestor::new(2);
        let client_state =
            create_test_client_state(vec![attestor1.eth_address, attestor2.eth_address], 2);
        let attestation_data = b"test data";

        let sig1 = attestor1.sign(attestation_data);
        let sig2 = attestor2.sign(attestation_data);

        let result = verify_attestation(&client_state, attestation_data, &[sig1, sig2]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_attestation_duplicate_signer_same_key() {
        let attestor = TestAttestor::new(1);
        let client_state = create_test_client_state(vec![attestor.eth_address], 2);
        let attestation_data = b"test data";

        // Same attestor signs twice - identical signatures recover to same address
        let sig = attestor.sign(attestation_data);
        let signatures = vec![sig.clone(), sig];

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_unknown_signer() {
        let trusted_attestor = TestAttestor::new(1);
        let unknown_attestor = TestAttestor::new(2);
        let client_state = create_test_client_state(vec![trusted_attestor.eth_address], 1);
        let attestation_data = b"test data";

        let sig = unknown_attestor.sign(attestation_data);

        let result = verify_attestation(&client_state, attestation_data, &[sig]);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_mixed_trusted_and_unknown() {
        let trusted_attestor = TestAttestor::new(1);
        let unknown_attestor = TestAttestor::new(2);
        let client_state = create_test_client_state(vec![trusted_attestor.eth_address], 2);
        let attestation_data = b"test data";

        let sig1 = trusted_attestor.sign(attestation_data);
        let sig2 = unknown_attestor.sign(attestation_data);

        let result = verify_attestation(&client_state, attestation_data, &[sig1, sig2]);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_attestation_single_signer_happy_path() {
        let attestor = TestAttestor::new(1);
        let client_state = create_test_client_state(vec![attestor.eth_address], 1);
        let attestation_data = b"test data";

        let sig = attestor.sign(attestation_data);

        let result = verify_attestation(&client_state, attestation_data, &[sig]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_attestation_three_of_five_quorum() {
        let attestors: Vec<_> = (1..=5).map(TestAttestor::new).collect();
        let addresses: Vec<_> = attestors.iter().map(|a| a.eth_address).collect();
        let client_state = create_test_client_state(addresses, 3);
        let attestation_data = b"test data";

        // Sign with only 3 of 5 attestors
        let signatures: Vec<_> = attestors[0..3]
            .iter()
            .map(|a| a.sign(attestation_data))
            .collect();

        let result = verify_attestation(&client_state, attestation_data, &signatures);
        assert!(result.is_ok());
    }
}
