//! Common test utilities and fixtures for uc-and-membership tests

#![allow(dead_code)] // Allow unused helper functions for future test expansion

use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ibc_client_tendermint::types::{ConsensusState, Header};
use ibc_core_client_types::Height;
use ibc_core_commitment_types::{commitment::CommitmentRoot, merkle::MerkleProof};
use ibc_proto::ibc::core::commitment::v1::MerkleProof as ProtoMerkleProof;
use prost::Message;
use tendermint_light_client_membership::KVPair;
use tendermint_light_client_uc_and_membership::{
    update_client_and_membership, UcAndMembershipError, UcAndMembershipOutput,
};
use tendermint_light_client_update_client::{ClientState, TrustThreshold};

/// Client state fixture structure from JSON
#[derive(Debug, Deserialize)]
pub struct ClientStateFixture {
    pub chain_id: String,
    pub frozen_height: u64,
    pub latest_height: u64,
    pub max_clock_drift: u64,
    pub trust_level_denominator: u32,
    pub trust_level_numerator: u32,
    pub trusting_period: u64,
    pub unbonding_period: u64,
}

/// Consensus state fixture structure from JSON
#[derive(Debug, Deserialize)]
pub struct ConsensusStateFixture {
    pub next_validators_hash: String,
    pub root: String,
    pub timestamp: u64,
}

/// Update client message fixture structure from JSON
#[derive(Debug, Deserialize, Clone)]
pub struct UpdateClientMessageFixture {
    pub client_message_hex: String,
}

/// Membership message fixture structure from JSON
#[derive(Debug, Clone, Deserialize)]
pub struct MembershipMsgFixture {
    pub path: Vec<String>,
    pub proof: String,
    pub value: String,
}

/// Combined update client and membership fixture from JSON
#[derive(Debug, Deserialize)]
pub struct UcAndMembershipFixture {
    pub scenario: String,
    pub client_state: ClientStateFixture,
    pub trusted_consensus_state: ConsensusStateFixture,
    pub update_client_message: UpdateClientMessageFixture,
    pub membership_msg: MembershipMsgFixture,
}

impl From<&ClientStateFixture> for ClientState {
    fn from(fixture: &ClientStateFixture) -> Self {
        Self {
            chain_id: fixture.chain_id.clone(),
            trust_level: TrustThreshold::new(
                fixture.trust_level_numerator as u64,
                fixture.trust_level_denominator as u64,
            ),
            trusting_period_seconds: fixture.trusting_period,
            unbonding_period_seconds: fixture.unbonding_period,
            max_clock_drift_seconds: fixture.max_clock_drift,
            is_frozen: fixture.frozen_height > 0,
            latest_height: Height::new(0, fixture.latest_height).expect("valid height"),
        }
    }
}

impl From<&MembershipMsgFixture> for KVPair {
    fn from(fixture: &MembershipMsgFixture) -> Self {
        let path_bytes: Vec<Vec<u8>> = fixture.path.iter().map(|s| s.as_bytes().to_vec()).collect();
        let value_bytes = hex::decode(&fixture.value).expect("valid hex");

        Self::new(path_bytes, value_bytes)
    }
}

/// Create a consensus state from fixture
pub fn consensus_state_from_fixture(
    fixture: &ConsensusStateFixture,
) -> Result<ConsensusState, Box<dyn std::error::Error>> {
    let root_bytes =
        hex::decode(&fixture.root).map_err(|e| format!("Failed to decode root hex: {}", e))?;
    let next_validators_hash_bytes = hex::decode(&fixture.next_validators_hash)
        .map_err(|e| format!("Failed to decode next_validators_hash hex: {}", e))?;

    let timestamp = tendermint::Time::from_unix_timestamp(
        (fixture.timestamp / 1_000_000_000) as i64,
        (fixture.timestamp % 1_000_000_000) as u32,
    )
    .map_err(|e| format!("Failed to create timestamp: {}", e))?;

    let next_validators_hash = tendermint::Hash::from_bytes(
        tendermint::hash::Algorithm::Sha256,
        &next_validators_hash_bytes,
    )
    .map_err(|e| format!("Failed to create next_validators_hash: {}", e))?;

    let commitment_root = CommitmentRoot::from_bytes(&root_bytes);

    Ok(ConsensusState::new(
        commitment_root,
        timestamp,
        next_validators_hash,
    ))
}

/// Convert hex string to Header by deserializing protobuf bytes
pub fn hex_to_header(hex_str: &str) -> Result<Header, Box<dyn std::error::Error>> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("Failed to decode header hex: {}", e))?;

    let proto_header = ibc_client_tendermint::types::proto::v1::Header::decode(&bytes[..])
        .map_err(|e| format!("Failed to decode protobuf header: {}", e))?;

    let header =
        Header::try_from(proto_header).map_err(|e| format!("Failed to convert header: {}", e))?;

    Ok(header)
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

/// Load a fixture from the fixtures directory
pub fn load_fixture(filename: &str) -> UcAndMembershipFixture {
    let fixture_path = Path::new("../fixtures").join(format!("{}.json", filename));
    let fixture_content = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", fixture_path.display()));

    serde_json::from_str(&fixture_content)
        .unwrap_or_else(|_| panic!("Failed to parse fixture: {}", fixture_path.display()))
}

/// Test context containing parsed fixture data
pub struct TestContext {
    pub fixture: UcAndMembershipFixture,
    pub client_state: ClientState,
    pub trusted_consensus_state: ConsensusState,
    pub proposed_header: Header,
    pub kv_pair: KVPair,
    pub merkle_proof: MerkleProof,
    pub current_time: u128,
}

/// Set up test context from fixture
pub fn setup_test_context(fixture: UcAndMembershipFixture) -> Option<TestContext> {
    let client_state = ClientState::from(&fixture.client_state);

    let trusted_consensus_state =
        match consensus_state_from_fixture(&fixture.trusted_consensus_state) {
            Ok(cs) => cs,
            Err(_) => {
                // Failed to create consensus state - return None for test to handle
                return None;
            }
        };

    let proposed_header = match hex_to_header(&fixture.update_client_message.client_message_hex) {
        Ok(header) => header,
        Err(_) => {
            // Failed to parse header - return None for test to handle
            return None;
        }
    };

    let kv_pair = KVPair::from(&fixture.membership_msg);
    let merkle_proof = hex_to_merkle_proof(&fixture.membership_msg.proof);

    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    Some(TestContext {
        fixture,
        client_state,
        trusted_consensus_state,
        proposed_header,
        kv_pair,
        merkle_proof,
        current_time,
    })
}

/// Execute update_client_and_membership with the test context
pub fn execute_uc_and_membership(
    ctx: &TestContext,
) -> Result<UcAndMembershipOutput, UcAndMembershipError> {
    let request = vec![(ctx.kv_pair.clone(), ctx.merkle_proof.clone())];
    update_client_and_membership(
        &ctx.client_state,
        &ctx.trusted_consensus_state,
        ctx.proposed_header.clone(),
        ctx.current_time,
        &request,
    )
}

/// Helper for tests expecting success
pub fn assert_uc_and_membership_success(ctx: &TestContext) {
    match execute_uc_and_membership(ctx) {
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
                "❌ Expected success but failed for {}: {:?}",
                ctx.fixture.scenario, e
            );
        }
    }
}

/// Helper for tests expecting failure
pub fn assert_uc_and_membership_failure(ctx: &TestContext) {
    match execute_uc_and_membership(ctx) {
        Ok(_) => {
            panic!(
                "❌ Expected failure but succeeded for {}",
                ctx.fixture.scenario
            );
        }
        Err(_) => {
            // Expected failure - test passes
        }
    }
}

/// Helper for tests expecting specific error types
pub fn assert_uc_and_membership_failure_with_error(
    ctx: &TestContext,
    expected_error_type: &str,
    description: &str,
) {
    match execute_uc_and_membership(ctx) {
        Ok(_) => {
            panic!(
                "❌ Expected {} error but succeeded for: {}",
                expected_error_type, description
            );
        }
        Err(e) => {
            let error_matches = matches!(
                (expected_error_type, &e),
                ("UpdateClient", UcAndMembershipError::UpdateClient(_))
                    | ("Membership", UcAndMembershipError::Membership(_))
                    | ("InvalidAppHash", UcAndMembershipError::InvalidAppHash(_))
            );

            assert!(
                error_matches,
                "Expected {} error but got different error for {}: {:?}",
                expected_error_type, description, e
            );
        }
    }
}

/// Generic helper to create a modified test context
fn create_modified_context<F>(fixture: UcAndMembershipFixture, modifier: F) -> Option<TestContext>
where
    F: FnOnce(&mut TestContext),
{
    let mut ctx = setup_test_context(fixture)?;
    modifier(&mut ctx);
    Some(ctx)
}

/// Helper to create a context with empty proof
pub fn create_context_with_empty_proof(fixture: UcAndMembershipFixture) -> Option<TestContext> {
    create_modified_context(fixture, |ctx| {
        ctx.merkle_proof = MerkleProof { proofs: vec![] };
    })
}

/// Helper to create a context with tampered value
pub fn create_context_with_tampered_value(fixture: UcAndMembershipFixture) -> Option<TestContext> {
    create_modified_context(fixture, |ctx| {
        ctx.kv_pair.value.push(0xFF); // Tamper with the value
    })
}

/// Helper to create a context with mismatched path
pub fn create_context_with_mismatched_path(
    fixture: UcAndMembershipFixture,
    new_path: Vec<Vec<u8>>,
) -> Option<TestContext> {
    create_modified_context(fixture, |ctx| {
        ctx.kv_pair.path = new_path;
    })
}

/// Helper to simulate malformed header by using invalid protobuf data
pub fn create_context_with_malformed_header(
    fixture: UcAndMembershipFixture,
) -> Option<TestContext> {
    // For malformed header, we'll try to modify the fixture before parsing
    let mut modified_fixture = fixture;

    // Corrupt the hex string to make it invalid
    let mut corrupted_hex = modified_fixture.update_client_message.client_message_hex;
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
    modified_fixture.update_client_message.client_message_hex = corrupted_hex;

    // This should fail during setup, which is expected behavior
    setup_test_context(modified_fixture)
}

/// Generic function to combine update client and membership fixtures
fn load_combined_fixture(
    update_client_filename: &str,
    membership_filename: &str,
    scenario_name: &str,
) -> UcAndMembershipFixture {
    let update_client_path =
        Path::new("../fixtures").join(format!("{}.json", update_client_filename));
    let membership_path = Path::new("../fixtures").join(format!("{}.json", membership_filename));

    let update_client_content = fs::read_to_string(&update_client_path).unwrap_or_else(|_| {
        panic!(
            "Failed to read update client fixture: {}",
            update_client_path.display()
        )
    });
    let membership_content = fs::read_to_string(&membership_path).unwrap_or_else(|_| {
        panic!(
            "Failed to read membership fixture: {}",
            membership_path.display()
        )
    });

    let update_client_fixture: serde_json::Value = serde_json::from_str(&update_client_content)
        .expect("Failed to parse update client fixture");
    let membership_fixture: serde_json::Value =
        serde_json::from_str(&membership_content).expect("Failed to parse membership fixture");

    let combined = serde_json::json!({
        "scenario": scenario_name,
        "client_state": update_client_fixture["client_state"],
        "trusted_consensus_state": update_client_fixture["trusted_consensus_state"],
        "update_client_message": update_client_fixture["update_client_message"],
        "membership_msg": membership_fixture["membership_msg"]
    });

    serde_json::from_value(combined).expect("Failed to deserialize combined fixture")
}

/// Load the combined update client and membership fixture for happy path
pub fn load_combined_happy_path_fixture() -> UcAndMembershipFixture {
    load_combined_fixture(
        "update_client_happy_path",
        "verify_membership_key_0",
        "uc_and_membership_happy_path",
    )
}

/// Load the combined fixture for expired header test
pub fn load_combined_expired_header_fixture() -> UcAndMembershipFixture {
    load_combined_fixture(
        "update_client_expired_header",
        "verify_membership_key_0",
        "uc_and_membership_expired_header",
    )
}

/// Load the combined fixture for malformed client message test
pub fn load_combined_malformed_message_fixture() -> UcAndMembershipFixture {
    load_combined_fixture(
        "update_client_malformed_client_message",
        "verify_membership_key_0",
        "uc_and_membership_malformed_message",
    )
}

/// Load combined fixture with valid update but invalid membership
pub fn load_combined_invalid_membership_fixture() -> UcAndMembershipFixture {
    load_combined_fixture(
        "update_client_happy_path",
        "verify_membership_key_0",
        "uc_and_membership_invalid_membership",
    )
}
