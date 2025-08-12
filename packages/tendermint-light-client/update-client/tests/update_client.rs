//! Integration tests for update client functionality

mod helpers;

use helpers::*;

#[test]
fn test_update_client_happy_path() {
    let fixture = load_happy_path_fixture();
    let ctx = setup_test_context(fixture);

    let output = execute_update_client(&ctx).expect("Expected success but failed for happy path");
    assert!(
        output.latest_height.revision_height() > output.trusted_height.revision_height(),
        "New height should be greater than trusted height"
    );
}

#[test]
fn test_update_client_malformed_message() {
    let fixture = load_malformed_client_message_fixture();
    let ctx = setup_test_context(fixture);

    execute_update_client(&ctx).expect_err(
        "Malformed message test should have failed but succeeded for malformed message",
    );
}

#[test]
fn test_update_client_expired_header() {
    let fixture = load_expired_header_fixture();
    let ctx = setup_test_context(fixture);

    execute_update_client(&ctx).expect_err("Expected failure but succeeded for expired header");
}

#[test]
fn test_update_client_future_timestamp() {
    let fixture = load_future_timestamp_fixture();
    let ctx = setup_test_context(fixture);

    execute_update_client(&ctx).expect_err("Expected failure but succeeded for future timestamp");
}
