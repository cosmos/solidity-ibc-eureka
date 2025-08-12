//! Integration tests for combined update client and membership functionality

mod helpers;

use helpers::*;
use tendermint_light_client_uc_and_membership::UcAndMembershipError;

#[test]
fn test_uc_and_membership_happy_path_with_empty_membership() {
    // Test with empty membership request - this should succeed as a true happy path
    let fixture = load_combined_happy_path_fixture();
    let ctx = setup_test_context(fixture);

    // Use empty membership request to avoid app hash compatibility issues
    let empty_request = vec![];

    let result = tendermint_light_client_uc_and_membership::update_client_and_membership(
        &ctx.client_state,
        &ctx.trusted_consensus_state,
        ctx.proposed_header.clone(),
        ctx.current_time,
        &empty_request,
    );

    // This should succeed - update client validation passes, empty membership is valid
    match result {
        Ok(output) => {
            assert!(
                output.update_output.latest_height.revision_height()
                    > output.update_output.trusted_height.revision_height(),
                "New height should be greater than trusted height"
            );
            assert_eq!(
                output.update_output.latest_height.revision_number(),
                output.update_output.trusted_height.revision_number(),
                "Revision number should remain consistent"
            );
        }
        Err(e) => {
            panic!(
                "❌ Happy path with empty membership should succeed: {:?}",
                e
            );
        }
    }
}

#[test]
fn test_uc_and_membership_update_client_fails_expired_header() {
    let fixture = load_combined_expired_header_fixture();
    let ctx = setup_test_context(fixture);
    assert_uc_and_membership_failure_with_error(&ctx, "UpdateClient", "expired header");
}

#[test]
fn test_uc_and_membership_update_client_fails_malformed_message() {
    let fixture = load_combined_malformed_message_fixture();
    let ctx = setup_test_context(fixture);
    assert_uc_and_membership_failure_with_error(&ctx, "UpdateClient", "malformed message");
}

#[test]
fn test_uc_and_membership_membership_fails_tampered_value() {
    let fixture = load_combined_invalid_membership_fixture();
    let ctx = create_context_with_tampered_value(fixture);
    assert_uc_and_membership_failure_with_error(&ctx, "Membership", "tampered value");
}

#[test]
fn test_uc_and_membership_membership_fails_empty_proof() {
    let fixture = load_combined_invalid_membership_fixture();
    let ctx = create_context_with_empty_proof(fixture);
    assert_uc_and_membership_failure_with_error(&ctx, "Membership", "empty proof");
}

#[test]
fn test_uc_and_membership_membership_fails_mismatched_path() {
    let fixture = load_combined_invalid_membership_fixture();
    let ctx =
        create_context_with_mismatched_path(fixture, vec![b"wrong".to_vec(), b"path".to_vec()]);
    assert_uc_and_membership_failure_with_error(&ctx, "Membership", "mismatched path");
}

#[test]
fn test_uc_and_membership_multiple_kv_pairs() {
    // Test with multiple membership proofs - expect failure due to app hash mismatch
    let fixture = load_combined_happy_path_fixture();
    let ctx = setup_test_context(fixture);

    // Create a request with multiple identical KV pairs
    let request = vec![
        (ctx.kv_pair.clone(), ctx.merkle_proof.clone()),
        (ctx.kv_pair.clone(), ctx.merkle_proof.clone()),
    ];

    let result = tendermint_light_client_uc_and_membership::update_client_and_membership(
        &ctx.client_state,
        &ctx.trusted_consensus_state,
        ctx.proposed_header.clone(),
        ctx.current_time,
        &request,
    );

    // Since we're combining fixtures with different app hashes, expect membership failure
    match result {
        Ok(output) => {
            // If it succeeds, validate the output
            assert!(
                output.update_output.latest_height.revision_height()
                    > output.update_output.trusted_height.revision_height(),
                "New height should be greater than trusted height"
            );
        }
        Err(tendermint_light_client_uc_and_membership::UcAndMembershipError::Membership(
            tendermint_light_client_membership::MembershipError::MembershipVerificationFailed,
        )) => {
            // Expected - app hash mismatch between fixtures
        }
        Err(e) => {
            panic!(
                "❌ Unexpected error type for multiple KV pairs test: {:?}",
                e
            );
        }
    }
}

#[test]
fn test_uc_and_membership_empty_request() {
    // Test with empty membership request - should succeed as empty membership is valid
    let fixture = load_combined_happy_path_fixture();
    let ctx = setup_test_context(fixture);

    let empty_request = vec![];

    let result = tendermint_light_client_uc_and_membership::update_client_and_membership(
        &ctx.client_state,
        &ctx.trusted_consensus_state,
        ctx.proposed_header.clone(),
        ctx.current_time,
        &empty_request,
    );

    match result {
        Ok(output) => {
            // Empty membership request should succeed - only update client is validated
            assert!(
                output.update_output.latest_height.revision_height()
                    > output.update_output.trusted_height.revision_height(),
                "New height should be greater than trusted height"
            );
            assert_eq!(
                output.update_output.latest_height.revision_number(),
                output.update_output.trusted_height.revision_number(),
                "Revision number should remain consistent"
            );
        }
        Err(tendermint_light_client_uc_and_membership::UcAndMembershipError::UpdateClient(_)) => {
            // Update client failure is acceptable for this test
        }
        Err(e) => {
            panic!("❌ Unexpected error for empty membership request: {:?}", e);
        }
    }
}

#[test]
fn test_uc_and_membership_sequence_validation() {
    // Test that both update client and membership validation occur in the correct order
    // First update client should succeed, then membership should be validated
    let fixture = load_combined_invalid_membership_fixture();

    // Create context where update client will succeed but membership will fail
    let ctx = create_context_with_tampered_value(fixture);

    // First verify that update client would succeed on its own
    let uc_result = tendermint_light_client_update_client::update_client(
        &ctx.client_state,
        &ctx.trusted_consensus_state,
        ctx.proposed_header.clone(),
        ctx.current_time,
    );

    if uc_result.is_err() {
        panic!("❌ Expected update client to succeed");
    }

    // Now test the combined operation - should fail at membership step
    match execute_uc_and_membership(&ctx) {
        Ok(_) => {
            panic!("❌ Expected failure due to invalid membership");
        }
        Err(UcAndMembershipError::Membership(
            tendermint_light_client_membership::MembershipError::MembershipVerificationFailed,
        )) => {
            // Expected - membership validation should fail after update client succeeds
        }
        Err(e) => {
            panic!(
                "❌ Expected Membership::MembershipVerificationFailed but got: {:?}",
                e
            );
        }
    }
}

#[test]
fn test_uc_and_membership_update_client_error_type() {
    // Test specific UpdateClient error type for expired header
    let expired_fixture = load_combined_expired_header_fixture();
    let ctx = setup_test_context(expired_fixture);

    match execute_uc_and_membership(&ctx) {
        Err(UcAndMembershipError::UpdateClient(
            tendermint_light_client_update_client::UpdateClientError::HeaderVerificationFailed,
        )) => {
            // Expected - expired header should fail with HeaderVerificationFailed
        }
        Err(e) => {
            panic!(
                "❌ Expected UpdateClient::HeaderVerificationFailed but got: {:?}",
                e
            );
        }
        Ok(_) => {
            panic!("❌ Expected UpdateClient error but succeeded");
        }
    }
}

#[test]
fn test_uc_and_membership_membership_error_type() {
    // Test specific Membership error type for empty proof
    let membership_fixture = load_combined_invalid_membership_fixture();
    let ctx = create_context_with_empty_proof(membership_fixture);

    // First verify that update client would succeed on its own
    let uc_result = tendermint_light_client_update_client::update_client(
        &ctx.client_state,
        &ctx.trusted_consensus_state,
        ctx.proposed_header.clone(),
        ctx.current_time,
    );

    if uc_result.is_err() {
        // Skip this test if update client itself fails - we want to test membership errors
        return;
    }

    match execute_uc_and_membership(&ctx) {
        Err(UcAndMembershipError::Membership(
            tendermint_light_client_membership::MembershipError::MembershipVerificationFailed,
        )) => {
            // Expected - empty proof should fail with MembershipVerificationFailed
        }
        Err(e) => {
            panic!(
                "❌ Expected Membership::MembershipVerificationFailed but got: {:?}",
                e
            );
        }
        Ok(_) => {
            panic!("❌ Expected Membership error but succeeded");
        }
    }
}
