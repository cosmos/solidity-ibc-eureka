//! Common test utilities and fixtures

#![allow(dead_code)] // Allow unused helper functions for future test expansion

use serde::Deserialize;
use std::fs;
use std::path::Path;

use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as ProtoMerkleProof;
use prost::Message;
use tendermint_light_client_membership::{membership, KVPair, MembershipError};

/// Membership message fixture structure from JSON
#[derive(Debug, Clone, Deserialize)]
pub struct MembershipMsgFixture {
    pub path: Vec<String>,
    pub proof: String,
    pub value: String,
}

/// Complete membership verification fixture from JSON
#[derive(Debug, Clone, Deserialize)]
pub struct MembershipVerificationFixture {
    pub client_state_hex: String,
    pub consensus_state_hex: String,
    pub membership_msg: MembershipMsgFixture,
    pub app_hash_hex: String,
}

impl From<&MembershipMsgFixture> for KVPair {
    fn from(fixture: &MembershipMsgFixture) -> Self {
        let path_bytes: Vec<Vec<u8>> = fixture.path.iter().map(|s| s.as_bytes().to_vec()).collect();
        let value_bytes = hex::decode(&fixture.value).expect("valid hex");

        Self::new(path_bytes, value_bytes)
    }
}

/// Convert hex string to MerkleProof using proper protobuf deserialization
pub fn hex_to_merkle_proof(hex_str: &str) -> MerkleProof {
    let bytes = hex::decode(hex_str).expect("valid hex");

    let proto_merkle_proof =
        ProtoMerkleProof::decode(bytes.as_slice()).expect("valid proto MerkleProof");
    proto_merkle_proof
        .try_into()
        .expect("valid conversion to MerkleProof")
}

/// Load a membership fixture from the fixtures directory
pub fn load_membership_fixture(filename: &str) -> MembershipVerificationFixture {
    let fixture_path = Path::new("../fixtures").join(format!("{}.json", filename));
    let fixture_content = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", fixture_path.display()));

    serde_json::from_str(&fixture_content)
        .unwrap_or_else(|_| panic!("Failed to parse fixture: {}", fixture_path.display()))
}

/// Load the membership fixture
pub fn load_membership_fixture_data() -> MembershipVerificationFixture {
    load_membership_fixture("verify_membership_key_0")
}

/// Load the non-membership fixture
pub fn load_non_membership_fixture_data() -> MembershipVerificationFixture {
    load_membership_fixture("verify_non-membership_key_1")
}

/// Test context containing parsed fixture data
pub struct TestContext {
    pub app_hash: [u8; 32],
    pub kv_pair: KVPair,
    pub merkle_proof: MerkleProof,
}

/// Set up test context from fixture
pub fn setup_test_context(fixture: MembershipVerificationFixture) -> TestContext {
    // Decode the app hash that was used for the proof
    let app_hash_bytes =
        hex::decode(&fixture.app_hash_hex).expect("Failed to decode app_hash_hex from fixture");

    assert_eq!(
        app_hash_bytes.len(),
        32,
        "App hash wrong length: {} bytes, expected 32",
        app_hash_bytes.len()
    );

    let mut app_hash = [0u8; 32];
    app_hash.copy_from_slice(&app_hash_bytes);

    let kv_pair = KVPair::from(&fixture.membership_msg);
    let merkle_proof = hex_to_merkle_proof(&fixture.membership_msg.proof);

    TestContext {
        app_hash,
        kv_pair,
        merkle_proof,
    }
}

/// Execute membership verification with the test context
pub fn execute_membership(ctx: &TestContext) -> Result<(), MembershipError> {
    let request = vec![(ctx.kv_pair.clone(), ctx.merkle_proof.clone())];
    membership(ctx.app_hash, &request)
}

/// Helper to assert that membership verification should fail with a specific error
pub fn assert_membership_fails_with(
    ctx: &TestContext,
    expected_error: MembershipError,
    test_description: &str,
) {
    match execute_membership(ctx) {
        Ok(()) => {
            panic!(
                "❌ Membership verification should have failed for: {}",
                test_description
            );
        }
        Err(actual_error) => match (expected_error, actual_error) {
            (
                MembershipError::MembershipVerificationFailed,
                MembershipError::MembershipVerificationFailed,
            ) => {
                println!(
                    "✅ Membership verification correctly failed: {}",
                    test_description
                );
            }
            (
                MembershipError::NonMembershipVerificationFailed,
                MembershipError::NonMembershipVerificationFailed,
            ) => {
                println!(
                    "✅ Non-membership verification correctly failed: {}",
                    test_description
                );
            }
            (expected, actual) => {
                panic!(
                    "❌ Expected {:?} but got {:?} for: {}",
                    expected, actual, test_description
                );
            }
        },
    }
}

/// Helper to assert that membership verification should succeed
pub fn assert_membership_succeeds(ctx: &TestContext, test_description: &str) {
    match execute_membership(ctx) {
        Ok(()) => {
            println!("✅ Membership verification succeeded: {}", test_description);
        }
        Err(e) => {
            panic!(
                "❌ Membership verification failed for {}: {:?}",
                test_description, e
            );
        }
    }
}

/// Helper to create a test context with wrong app hash
pub fn create_context_with_wrong_app_hash(fixture: MembershipVerificationFixture) -> TestContext {
    let mut ctx = setup_test_context(fixture);
    ctx.app_hash = [0xFF; 32]; // Use a completely different app hash
    ctx
}

/// Helper to create a test context with empty proof
pub fn create_context_with_empty_proof(fixture: MembershipVerificationFixture) -> TestContext {
    let mut ctx = setup_test_context(fixture);
    ctx.merkle_proof = MerkleProof { proofs: vec![] };
    ctx
}

/// Helper to create a test context with mismatched path
pub fn create_context_with_mismatched_path(
    fixture: MembershipVerificationFixture,
    new_path: Vec<Vec<u8>>,
) -> TestContext {
    let mut ctx = setup_test_context(fixture);
    ctx.kv_pair.path = new_path;
    ctx
}

/// Helper to create a test context with tampered value
pub fn create_context_with_tampered_value(fixture: MembershipVerificationFixture) -> TestContext {
    let mut ctx = setup_test_context(fixture);
    ctx.kv_pair.value.push(0xFF); // Tamper with the value
    ctx
}

/// Helper to create a test context where membership is treated as non-membership
pub fn create_context_membership_as_non_membership(
    fixture: MembershipVerificationFixture,
) -> TestContext {
    let mut ctx = setup_test_context(fixture);
    ctx.kv_pair.value.clear(); // Clear value to make it look like non-membership
    ctx
}

/// Helper to create a test context where non-membership is treated as membership
pub fn create_context_non_membership_as_membership(
    fixture: MembershipVerificationFixture,
    fake_value: Vec<u8>,
) -> TestContext {
    let mut ctx = setup_test_context(fixture);
    ctx.kv_pair.value = fake_value; // Add fake value to make it look like membership
    ctx
}

/// Helper to create a test context with different proof
pub fn create_context_with_different_proof(
    mut ctx: TestContext,
    other_fixture: MembershipVerificationFixture,
) -> TestContext {
    ctx.merkle_proof = hex_to_merkle_proof(&other_fixture.membership_msg.proof);
    ctx
}

/// Helper to create a malformed proof by corrupting one character in the hex string
fn create_malformed_proof_hex(original_hex: &str) -> String {
    let mut corrupted_hex = original_hex.to_string();
    if let Some(first_char) = corrupted_hex.chars().next() {
        let corrupted_char = match first_char {
            '0' => '1',
            '1' => '0',
            'a' => 'b',
            'b' => 'a',
            'f' => 'e',
            'e' => 'f',
            _ => '0',
        };
        corrupted_hex.replace_range(0..1, &corrupted_char.to_string());
    }
    corrupted_hex
}

/// Helper to create a test context with a malformed proof (corrupted hex)
pub fn create_context_with_malformed_proof(fixture: MembershipVerificationFixture) -> TestContext {
    let mut ctx = setup_test_context(fixture.clone());

    // Create malformed proof by corrupting one hex character
    let malformed_hex = create_malformed_proof_hex(&fixture.membership_msg.proof);

    // Try to decode the malformed proof - this should fail during hex decoding or protobuf parsing
    // If hex decoding fails, we'll create an empty proof instead to test protobuf validation
    match hex::decode(&malformed_hex) {
        Ok(bytes) => {
            // If hex decoding succeeds, try to parse as protobuf (this should fail)
            match ProtoMerkleProof::decode(bytes.as_slice()) {
                Ok(proto_proof) => {
                    // If protobuf parsing somehow succeeds, try the final conversion
                    if let Ok(merkle_proof) = proto_proof.try_into() {
                        ctx.merkle_proof = merkle_proof;
                    } else {
                        // Conversion failed - use empty proof to force an error
                        ctx.merkle_proof = MerkleProof { proofs: vec![] };
                    }
                }
                Err(_) => {
                    // Protobuf parsing failed - use empty proof to force an error
                    ctx.merkle_proof = MerkleProof { proofs: vec![] };
                }
            }
        }
        Err(_) => {
            // Hex decoding failed - use empty proof to force an error
            ctx.merkle_proof = MerkleProof { proofs: vec![] };
        }
    }

    ctx
}
