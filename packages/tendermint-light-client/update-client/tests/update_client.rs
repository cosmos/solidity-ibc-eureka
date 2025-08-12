//! Integration tests for update client functionality

mod helpers;

use helpers::*;

#[test]
fn test_update_client_happy_path() {
    let fixture = load_happy_path_fixture();
    let ctx = setup_test_context(fixture);
    assert_update_success(&ctx, "happy path");
}

#[test]
fn test_update_client_malformed_message() {
    let fixture = load_malformed_client_message_fixture();
    let ctx = setup_test_context(fixture);
    assert_malformed_failure(&ctx, "malformed message");
}

#[test]
fn test_update_client_expired_header() {
    let fixture = load_expired_header_fixture();
    let ctx = setup_test_context(fixture);
    assert_update_failure(&ctx, "expired header");
}

#[test]
fn test_update_client_future_timestamp() {
    let fixture = load_future_timestamp_fixture();
    let ctx = setup_test_context(fixture);
    assert_update_failure(&ctx, "future timestamp");
}
