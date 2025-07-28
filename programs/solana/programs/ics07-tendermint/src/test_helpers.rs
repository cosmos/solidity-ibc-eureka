#[cfg(test)]
pub mod fixtures {
    use crate::types::{ClientState, ConsensusState, IbcHeight};
    use serde::Deserialize;

    #[derive(Deserialize)]
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

    #[derive(Deserialize)]
    pub struct ConsensusStateFixture {
        pub next_validators_hash: String,
        pub root: String,
        pub timestamp: u64,
    }

    #[derive(Deserialize, Clone)]
    pub struct UpdateClientMessageFixture {
        pub client_message_hex: String,
        pub type_url: String,
        pub trusted_height: u64,
        pub new_height: u64,
    }

    #[derive(Deserialize)]
    pub struct FixtureMetadata {
        pub description: String,
        pub generated_at: String,
        pub source: String,
    }

    // New unified fixture structure
    #[derive(Deserialize)]
    pub struct UnifiedUpdateClientFixture {
        pub scenario: String,
        pub client_state: ClientStateFixture,
        pub trusted_consensus_state: ConsensusStateFixture,
        pub update_client_message: UpdateClientMessageFixture,
        pub metadata: FixtureMetadata,
    }

    // Legacy individual fixture loaders - kept for potential future use with other scenarios
    pub fn load_client_state_fixture() -> ClientStateFixture {
        let unified = load_unified_happy_path_fixture();
        unified.client_state
    }

    pub fn load_consensus_state_fixture() -> ConsensusStateFixture {
        let unified = load_unified_happy_path_fixture();
        unified.trusted_consensus_state
    }

    pub fn load_update_client_message_fixture() -> UpdateClientMessageFixture {
        let unified = load_unified_happy_path_fixture();
        unified.update_client_message
    }

    // New unified fixture loaders
    pub fn load_unified_update_client_fixture(scenario: &str) -> UnifiedUpdateClientFixture {
        let fixture_str = match scenario {
            "happy_path" => include_str!("../../../tests/fixtures/update_client_happy_path.json"),
            "malformed_client_message" => {
                include_str!("../../../tests/fixtures/update_client_malformed_client_message.json")
            }
            _ => {
                // For other scenarios, try to read from filesystem
                let fixture_path =
                    format!("../../../tests/fixtures/update_client_{}.json", scenario);
                &std::fs::read_to_string(&fixture_path)
                    .unwrap_or_else(|_| panic!("Failed to read unified fixture: {}", fixture_path))
            }
        };

        serde_json::from_str(fixture_str).unwrap_or_else(|_| {
            panic!("Failed to parse unified fixture for scenario: {}", scenario)
        })
    }

    pub fn load_unified_happy_path_fixture() -> UnifiedUpdateClientFixture {
        load_unified_update_client_fixture("happy_path")
    }

    pub fn load_unified_malformed_client_message_fixture() -> UnifiedUpdateClientFixture {
        load_unified_update_client_fixture("malformed_client_message")
    }

    // Efficient function to load all primary fixtures at once
    pub fn load_primary_fixtures() -> (ClientState, ConsensusState, UpdateClientMessageFixture) {
        let unified = load_unified_happy_path_fixture();
        (
            extract_client_state_from_unified(&unified),
            extract_consensus_state_from_unified(&unified),
            extract_update_message_from_unified(&unified),
        )
    }

    // Primary fixture loading function for backward compatibility (still used in some tests)
    // This parses JSON each time - prefer using load_primary_fixtures() for efficiency
    pub fn load_primary_update_client_message() -> UpdateClientMessageFixture {
        let unified = load_unified_happy_path_fixture();
        extract_update_message_from_unified(&unified)
    }

    // Helper functions to extract components from unified fixture for backward compatibility
    pub fn extract_client_state_from_unified(unified: &UnifiedUpdateClientFixture) -> ClientState {
        client_state_from_fixture(&unified.client_state)
    }

    pub fn extract_consensus_state_from_unified(
        unified: &UnifiedUpdateClientFixture,
    ) -> ConsensusState {
        consensus_state_from_fixture(&unified.trusted_consensus_state)
    }

    pub fn extract_update_message_from_unified(
        unified: &UnifiedUpdateClientFixture,
    ) -> UpdateClientMessageFixture {
        unified.update_client_message.clone()
    }

    // Helper function to load available test scenarios
    pub fn load_available_scenarios() -> Vec<String> {
        // For now, return a hardcoded list. In the future, this could dynamically scan the fixtures directory
        vec![
            "happy_path".to_string(),
            // Add more scenarios as they're implemented
        ]
    }

    // Helper to check if a scenario exists
    pub fn scenario_exists(scenario: &str) -> bool {
        let fixture_path = format!("../../../tests/fixtures/update_client_{}.json", scenario);
        std::path::Path::new(&fixture_path).exists()
    }

    pub fn hex_to_bytes32(hex_str: &str) -> [u8; 32] {
        let hex_str = hex_str.trim_start_matches("0x");
        let bytes = hex::decode(hex_str).expect("Invalid hex string");
        let mut array = [0u8; 32];
        array.copy_from_slice(&bytes[..32]);
        array
    }

    pub fn hex_to_bytes(hex_str: &str) -> Vec<u8> {
        let hex_str = hex_str.trim_start_matches("0x");
        hex::decode(hex_str).expect("Invalid hex string")
    }

    pub fn client_state_from_fixture(fixture: &ClientStateFixture) -> ClientState {
        ClientState {
            chain_id: fixture.chain_id.clone(),
            trust_level_numerator: fixture.trust_level_numerator as u64,
            trust_level_denominator: fixture.trust_level_denominator as u64,
            trusting_period: fixture.trusting_period,
            unbonding_period: fixture.unbonding_period,
            max_clock_drift: fixture.max_clock_drift,
            frozen_height: IbcHeight {
                revision_number: 0,
                revision_height: fixture.frozen_height,
            },
            latest_height: IbcHeight {
                revision_number: 0,
                revision_height: fixture.latest_height,
            },
        }
    }

    pub fn consensus_state_from_fixture(fixture: &ConsensusStateFixture) -> ConsensusState {
        ConsensusState {
            timestamp: fixture.timestamp,
            root: hex_to_bytes32(&fixture.root),
            next_validators_hash: hex_to_bytes32(&fixture.next_validators_hash),
        }
    }

    /// Extract header timestamp from update client message fixture
    /// Returns the header time as Unix timestamp in seconds (suitable for Clock sysvar)
    pub fn get_header_timestamp_from_fixture(fixture: &UpdateClientMessageFixture) -> i64 {
        use crate::helpers::deserialize_header;

        let client_message = hex_to_bytes(&fixture.client_message_hex);
        let header =
            deserialize_header(&client_message).expect("Failed to deserialize header from fixture");

        // Extract timestamp from header and convert to Unix seconds
        let header_time_nanos = header.signed_header.header.time.unix_timestamp_nanos() as u64;
        (header_time_nanos / 1_000_000_000) as i64
    }

    /// Create a clock timestamp that's valid for the given header
    /// This adds a small buffer to the header time to pass clock drift validation
    pub fn get_valid_clock_timestamp_for_header(fixture: &UpdateClientMessageFixture) -> i64 {
        let header_timestamp = get_header_timestamp_from_fixture(fixture);
        // Add 5 seconds buffer after header time to pass validation
        header_timestamp + 5
    }

    /// Create a clock timestamp that's way in the future to simulate expired header
    /// This is used for testing header expiration scenarios
    pub fn get_expired_clock_timestamp_for_header(fixture: &UpdateClientMessageFixture) -> i64 {
        let header_timestamp = get_header_timestamp_from_fixture(fixture);
        // Add 1 year to make the header appear expired
        header_timestamp + 86400 * 365
    }
}
