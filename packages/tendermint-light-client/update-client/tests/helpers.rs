//! Common test utilities and fixtures for update client tests

#![allow(dead_code)] // Allow unused helper functions for future test expansion

use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ibc_client_tendermint::types::{ConsensusState, Header};
use ibc_core_client_types::Height;
use ibc_core_commitment_types::commitment::CommitmentRoot;
use prost::Message;
use tendermint_light_client_update_client::{
    update_client, ClientState, TrustThreshold, UpdateClientError,
};

/// Update client message fixture structure from JSON
#[derive(Debug, Deserialize, Clone)]
pub struct UpdateClientMessageFixture {
    pub client_message_hex: String,
}

/// Complete update client fixture from JSON
#[derive(Debug, Deserialize)]
pub struct UpdateClientFixture {
    pub client_state_hex: String,
    pub consensus_state_hex: String,
    pub update_client_message: UpdateClientMessageFixture,
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

/// Load a fixture from the fixtures directory
pub fn load_fixture(filename: &str) -> UpdateClientFixture {
    let fixture_path = Path::new("../fixtures").join(format!("{}.json", filename));
    let fixture_content = fs::read_to_string(&fixture_path)
        .unwrap_or_else(|_| panic!("Failed to read fixture: {}", fixture_path.display()));

    serde_json::from_str(&fixture_content)
        .unwrap_or_else(|_| panic!("Failed to parse fixture: {}", fixture_path.display()))
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

/// Test context containing parsed fixture data
pub struct TestContext {
    pub client_state: ClientState,
    pub trusted_consensus_state: ConsensusState,
    pub proposed_header: Header,
    pub current_time: u128,
}

/// Set up test context from fixture
pub fn setup_test_context(fixture: UpdateClientFixture) -> TestContext {
    let client_state = client_state_from_hex(&fixture.client_state_hex)
        .expect("Failed to create client state from fixture");

    let trusted_consensus_state = consensus_state_from_hex(&fixture.consensus_state_hex)
        .expect("Failed to create consensus state from fixture");

    let proposed_header = hex_to_header(&fixture.update_client_message.client_message_hex)
        .expect("Failed to parse header from fixture");

    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    TestContext {
        client_state,
        trusted_consensus_state,
        proposed_header,
        current_time,
    }
}

/// Execute update_client with the test context
pub fn execute_update_client(
    ctx: &TestContext,
) -> Result<tendermint_light_client_update_client::UpdateClientOutput, UpdateClientError> {
    update_client(
        &ctx.client_state,
        &ctx.trusted_consensus_state,
        ctx.proposed_header.clone(),
        ctx.current_time,
    )
}

/// Helper for tests expecting success
pub fn assert_update_success(ctx: &TestContext, scenario_name: &str) {
    match execute_update_client(ctx) {
        Ok(output) => {
            println!("✅ Update client succeeded for {}", scenario_name);
            println!("   New height: {:?}", output.latest_height);
            println!("   Trusted height: {:?}", output.trusted_height);
            assert!(
                output.latest_height.revision_height() > output.trusted_height.revision_height(),
                "New height should be greater than trusted height"
            );
        }
        Err(e) => {
            panic!(
                "❌ Expected success but failed for {}: {:?}",
                scenario_name, e
            );
        }
    }
}

/// Helper for tests expecting failure
pub fn assert_update_failure(ctx: &TestContext, scenario_name: &str) {
    match execute_update_client(ctx) {
        Ok(_) => {
            panic!("❌ Expected failure but succeeded for {}", scenario_name);
        }
        Err(e) => {
            println!(
                "✅ Update client correctly failed for {} with: {:?}",
                scenario_name, e
            );
        }
    }
}

/// Helper for malformed message test with specific error handling
pub fn assert_malformed_failure(ctx: &TestContext, scenario_name: &str) {
    match execute_update_client(ctx) {
        Ok(_) => {
            panic!(
                "❌ Malformed message test should have failed but succeeded for {}",
                scenario_name
            );
        }
        Err(UpdateClientError::HeaderVerificationFailed) => {
            println!(
                "✅ Update client correctly failed with HeaderVerificationFailed for {}",
                scenario_name
            );
        }
        Err(e) => {
            println!(
                "✅ Update client failed for {} with: {:?}",
                scenario_name, e
            );
            // Other errors are also acceptable for malformed messages
        }
    }
}

/// Load the happy path fixture
pub fn load_happy_path_fixture() -> UpdateClientFixture {
    load_fixture("update_client_happy_path")
}

/// Load the malformed client message fixture
pub fn load_malformed_client_message_fixture() -> UpdateClientFixture {
    load_fixture("update_client_malformed_client_message")
}

/// Load the expired header fixture
pub fn load_expired_header_fixture() -> UpdateClientFixture {
    load_fixture("update_client_expired_header")
}

/// Load the future timestamp fixture
pub fn load_future_timestamp_fixture() -> UpdateClientFixture {
    load_fixture("update_client_future_timestamp")
}

/// Load the invalid protobuf fixture
pub fn load_invalid_protobuf_fixture() -> UpdateClientFixture {
    load_fixture("update_client_invalid_protobuf")
}
