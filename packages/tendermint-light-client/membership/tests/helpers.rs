//! Common test utilities and fixtures

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

/// Extension trait for parsing from hex
trait ParseFromHex: Sized {
    fn from_hex(hex_str: &str) -> Result<Self, Box<dyn std::error::Error>>;
}

impl ParseFromHex for MerkleProof {
    fn from_hex(hex_str: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let bytes = hex::decode(hex_str)
            .map_err(|e| format!("Failed to decode merkle proof hex: {}", e))?;

        let proto_merkle_proof = ProtoMerkleProof::decode(bytes.as_slice())
            .map_err(|e| format!("Failed to decode protobuf merkle proof: {}", e))?;
        
        proto_merkle_proof.try_into()
            .map_err(|e| format!("Failed to convert merkle proof: {:?}", e).into())
    }
}

/// Convert hex string to MerkleProof (backward compatibility wrapper)
pub fn hex_to_merkle_proof(hex_str: &str) -> MerkleProof {
    MerkleProof::from_hex(hex_str).expect("valid merkle proof")
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

    let app_hash: [u8; 32] = app_hash_bytes
        .try_into()
        .expect("App hash must be exactly 32 bytes");

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
    let actual_error = execute_membership(ctx).expect_err(&format!(
        "Membership verification should have failed for: {}",
        test_description
    ));

    assert!(
        std::mem::discriminant(&expected_error) == std::mem::discriminant(&actual_error),
        "Expected {:?} but got {:?} for: {}",
        expected_error,
        actual_error,
        test_description
    );
}

/// Helper to assert that membership verification should succeed
pub fn assert_membership_succeeds(ctx: &TestContext, test_description: &str) {
    execute_membership(ctx).expect(&format!(
        "Membership verification failed for {}",
        test_description
    ));
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
    let mut s = original_hex.to_string();
    s.replace_range(0..1, if &s[0..1] == "0" { "1" } else { "0" });
    s
}

/// Helper to create a test context with a malformed proof (corrupted hex)
pub fn create_context_with_malformed_proof(fixture: MembershipVerificationFixture) -> TestContext {
    let mut ctx = setup_test_context(fixture.clone());

    let malformed_hex = create_malformed_proof_hex(&fixture.membership_msg.proof);

    ctx.merkle_proof = hex::decode(&malformed_hex)
        .ok()
        .and_then(|bytes| ProtoMerkleProof::decode(bytes.as_slice()).ok())
        .and_then(|proto| proto.try_into().ok())
        .unwrap_or_else(|| MerkleProof { proofs: vec![] });

    ctx
}

