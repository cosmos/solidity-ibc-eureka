//! Tests for acknowledgement packet handling

use super::*;
use sha2::{Digest, Sha256};

fn universal_error_ack() -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"UNIVERSAL_ERROR_ACKNOWLEDGEMENT");
    let result = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&result);
    bytes
}

#[test]
fn test_parse_gmp_acknowledgement_error() {
    let error_ack = universal_error_ack();
    assert!(
        !parse_gmp_acknowledgement(&error_ack),
        "UNIVERSAL_ERROR_ACK should indicate failure"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_success() {
    let success_ack = b"some protobuf encoded acknowledgement";
    assert!(
        parse_gmp_acknowledgement(success_ack),
        "Non-error ack should indicate success"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_empty() {
    let empty_ack: &[u8] = &[];
    assert!(
        parse_gmp_acknowledgement(empty_ack),
        "Empty ack should indicate success (not error)"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_partial_match() {
    let error_ack = universal_error_ack();
    let partial_ack = &error_ack[..31];
    assert!(
        parse_gmp_acknowledgement(partial_ack),
        "Partial match should indicate success"
    );
}

#[test]
fn test_parse_gmp_acknowledgement_extended() {
    let error_ack = universal_error_ack();
    let mut extended = error_ack.to_vec();
    extended.push(0);
    assert!(
        parse_gmp_acknowledgement(&extended),
        "Extended error ack should indicate success"
    );
}
