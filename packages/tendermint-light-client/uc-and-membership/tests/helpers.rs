//! Common test utilities and fixtures for uc-and-membership tests

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

/// Update client fixture structure from JSON
#[derive(Debug, Deserialize)]
struct UpdateClientFixture {
    client_state_hex: String,
    consensus_state_hex: String,
    update_client_message: UpdateClientMessageFixture,
}

/// Membership verification fixture structure from JSON
#[derive(Debug, Deserialize)]
struct MembershipFixture {
    membership_msg: MembershipMsgFixture,
}

/// Combined update client and membership fixture (created by merging separate fixtures)
#[derive(Debug, Deserialize)]
pub struct UcAndMembershipFixture {
    pub client_state_hex: String,
    pub consensus_state_hex: String,
    pub update_client_message: UpdateClientMessageFixture,
    pub membership_msg: MembershipMsgFixture,
}

impl From<&MembershipMsgFixture> for KVPair {
    fn from(fixture: &MembershipMsgFixture) -> Self {
        let path_bytes: Vec<Vec<u8>> = fixture.path.iter().map(|s| s.as_bytes().to_vec()).collect();
        let value_bytes = hex::decode(&fixture.value).expect("valid hex");

        Self::new(path_bytes, value_bytes)
    }
}

/// Parse a client state from hex-encoded protobuf
pub fn client_state_from_hex(hex_str: &str) -> Result<ClientState, Box<dyn std::error::Error>> {
    let bytes =
        hex::decode(hex_str).map_err(|e| format!("Failed to decode client state hex: {}", e))?;

    let proto_client_state =
        ibc_client_tendermint::types::proto::v1::ClientState::decode(&bytes[..])
            .map_err(|e| format!("Failed to decode protobuf client state: {}", e))?;

    // Parse protobuf fields
    let trust_level = proto_client_state
        .trust_level
        .ok_or("Missing trust level in client state")?;

    let trusting_period = proto_client_state
        .trusting_period
        .ok_or("Missing trusting period in client state")?;
    let unbonding_period = proto_client_state
        .unbonding_period
        .ok_or("Missing unbonding period in client state")?;
    let max_clock_drift = proto_client_state
        .max_clock_drift
        .ok_or("Missing max clock drift in client state")?;

    let latest_height = proto_client_state
        .latest_height
        .ok_or("Missing latest height in client state")?;

    Ok(ClientState {
        chain_id: proto_client_state.chain_id,
        trust_level: TrustThreshold::new(trust_level.numerator, trust_level.denominator),
        trusting_period_seconds: trusting_period.seconds as u64,
        unbonding_period_seconds: unbonding_period.seconds as u64,
        max_clock_drift_seconds: max_clock_drift.seconds as u64,
        is_frozen: proto_client_state.frozen_height.is_some(),
        latest_height: Height::new(latest_height.revision_number, latest_height.revision_height)
            .map_err(|e| format!("Invalid height: {}", e))?,
    })
}

/// Parse a consensus state from hex-encoded protobuf
pub fn consensus_state_from_hex(
    hex_str: &str,
) -> Result<ConsensusState, Box<dyn std::error::Error>> {
    let bytes =
        hex::decode(hex_str).map_err(|e| format!("Failed to decode consensus state hex: {}", e))?;

    let proto_consensus_state =
        ibc_client_tendermint::types::proto::v1::ConsensusState::decode(&bytes[..])
            .map_err(|e| format!("Failed to decode protobuf consensus state: {}", e))?;

    let timestamp = proto_consensus_state
        .timestamp
        .ok_or("Missing timestamp in consensus state")?;
    let root = proto_consensus_state
        .root
        .ok_or("Missing root in consensus state")?;

    let tm_timestamp =
        tendermint::Time::from_unix_timestamp(timestamp.seconds, timestamp.nanos as u32)
            .map_err(|e| format!("Failed to create timestamp: {}", e))?;

    let next_validators_hash = tendermint::Hash::from_bytes(
        tendermint::hash::Algorithm::Sha256,
        &proto_consensus_state.next_validators_hash,
    )
    .map_err(|e| format!("Failed to create next_validators_hash: {}", e))?;

    let commitment_root = CommitmentRoot::from_bytes(&root.hash);

    Ok(ConsensusState::new(
        commitment_root,
        tm_timestamp,
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


/// Test context containing parsed fixture data
pub struct TestContext {
    pub client_state: ClientState,
    pub trusted_consensus_state: ConsensusState,
    pub proposed_header: Header,
    pub kv_pair: KVPair,
    pub merkle_proof: MerkleProof,
    pub current_time: u128,
}

/// Set up test context from fixture
pub fn setup_test_context(fixture: UcAndMembershipFixture) -> TestContext {
    let client_state = client_state_from_hex(&fixture.client_state_hex)
        .expect("Failed to create client state from fixture");

    let trusted_consensus_state = consensus_state_from_hex(&fixture.consensus_state_hex)
        .expect("Failed to create consensus state from fixture");

    let proposed_header = hex_to_header(&fixture.update_client_message.client_message_hex)
        .expect("Failed to parse header from fixture");

    let kv_pair = KVPair::from(&fixture.membership_msg);
    let merkle_proof = hex_to_merkle_proof(&fixture.membership_msg.proof);

    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    TestContext {
        client_state,
        trusted_consensus_state,
        proposed_header,
        kv_pair,
        merkle_proof,
        current_time,
    }
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
fn create_modified_context<F>(fixture: UcAndMembershipFixture, modifier: F) -> TestContext
where
    F: FnOnce(&mut TestContext),
{
    let mut ctx = setup_test_context(fixture);
    modifier(&mut ctx);
    ctx
}

/// Helper to create a context with empty proof
pub fn create_context_with_empty_proof(fixture: UcAndMembershipFixture) -> TestContext {
    create_modified_context(fixture, |ctx| {
        ctx.merkle_proof = MerkleProof { proofs: vec![] };
    })
}

/// Helper to create a context with tampered value
pub fn create_context_with_tampered_value(fixture: UcAndMembershipFixture) -> TestContext {
    create_modified_context(fixture, |ctx| {
        ctx.kv_pair.value.push(0xFF); // Tamper with the value
    })
}

/// Helper to create a context with mismatched path
pub fn create_context_with_mismatched_path(
    fixture: UcAndMembershipFixture,
    new_path: Vec<Vec<u8>>,
) -> TestContext {
    create_modified_context(fixture, |ctx| {
        ctx.kv_pair.path = new_path;
    })
}

/// Generic function to combine update client and membership fixtures
fn load_combined_fixture(
    update_client_filename: &str,
    membership_filename: &str,
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

    let update_client_fixture: UpdateClientFixture = serde_json::from_str(&update_client_content)
        .expect("Failed to parse update client fixture");
    let membership_fixture: MembershipFixture =
        serde_json::from_str(&membership_content).expect("Failed to parse membership fixture");

    // Use the update client fixture's client and consensus state (they should be consistent)
    UcAndMembershipFixture {
        client_state_hex: update_client_fixture.client_state_hex,
        consensus_state_hex: update_client_fixture.consensus_state_hex,
        update_client_message: update_client_fixture.update_client_message,
        membership_msg: membership_fixture.membership_msg,
    }
}

/// Load the combined update client and membership fixture for happy path
pub fn load_combined_happy_path_fixture() -> UcAndMembershipFixture {
    load_combined_fixture("update_client_happy_path", "verify_membership_key_0")
}

/// Load the combined fixture for expired header test
pub fn load_combined_expired_header_fixture() -> UcAndMembershipFixture {
    load_combined_fixture("update_client_expired_header", "verify_membership_key_0")
}

/// Load the combined fixture for malformed client message test
pub fn load_combined_malformed_message_fixture() -> UcAndMembershipFixture {
    load_combined_fixture(
        "update_client_malformed_client_message",
        "verify_membership_key_0",
    )
}

/// Load combined fixture with valid update but invalid membership
pub fn load_combined_invalid_membership_fixture() -> UcAndMembershipFixture {
    load_combined_fixture("update_client_happy_path", "verify_membership_key_0")
}
