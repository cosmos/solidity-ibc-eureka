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

    #[derive(Deserialize)]
    pub struct UpdateClientFixture {
        pub scenario: String,
        pub client_state: ClientStateFixture,
        pub trusted_consensus_state: ConsensusStateFixture,
        pub update_client_message: UpdateClientMessageFixture,
        pub metadata: FixtureMetadata,
    }


    pub fn load_update_client_fixture(filename: &str) -> UpdateClientFixture {
        let fixture_path = format!("../../tests/fixtures/{}.json", filename);
        let fixture_content = std::fs::read_to_string(&fixture_path)
            .unwrap_or_else(|_| panic!("Failed to read fixture: {}", fixture_path));
        
        serde_json::from_str(&fixture_content)
            .unwrap_or_else(|_| panic!("Failed to parse fixture: {}", fixture_path))
    }

    pub fn load_happy_path_fixture() -> UpdateClientFixture {
        load_update_client_fixture("update_client_happy_path")
    }

    pub fn load_malformed_client_message_fixture() -> UpdateClientFixture {
        load_update_client_fixture("update_client_malformed_client_message")
    }

    pub fn load_expired_header_fixture() -> UpdateClientFixture {
        load_update_client_fixture("update_client_expired_header")
    }

    pub fn load_future_timestamp_fixture() -> UpdateClientFixture {
        load_update_client_fixture("update_client_future_timestamp")
    }

    pub fn load_wrong_trusted_height_fixture() -> UpdateClientFixture {
        load_update_client_fixture("update_client_wrong_trusted_height")
    }

    pub fn load_invalid_protobuf_fixture() -> UpdateClientFixture {
        load_update_client_fixture("update_client_invalid_protobuf")
    }

    pub fn load_primary_fixtures() -> (ClientState, ConsensusState, UpdateClientMessageFixture) {
        let fixture = load_happy_path_fixture();
        (
            client_state_from_fixture(&fixture.client_state),
            consensus_state_from_fixture(&fixture.trusted_consensus_state),
            fixture.update_client_message.clone(),
        )
    }

    // Helper to check if a fixture file exists
    pub fn fixture_exists(filename: &str) -> bool {
        let fixture_path = format!("../../tests/fixtures/{}.json", filename);
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

    // Generic test helper functions
    pub fn get_error_code(error: &anchor_lang::prelude::ProgramError) -> Option<u32> {
        match error {
            anchor_lang::prelude::ProgramError::Custom(code) => Some(*code),
            _ => None,
        }
    }

    pub fn assert_error_code(result: mollusk_svm::result::InstructionResult, expected_error: crate::error::ErrorCode, test_name: &str) {
        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                panic!("Expected {} to fail with {:?}, but it succeeded", test_name, expected_error);
            }
            mollusk_svm::result::ProgramResult::Failure(error) => {
                if let Some(code) = get_error_code(&error) {
                    let expected_code = expected_error as u32 + 6000; // Anchor errors start at 6000
                    assert_eq!(code, expected_code, 
                        "Expected {:?} ({}), but got error code {}", 
                        expected_error, expected_code, code);
                    println!("✅ {} correctly failed with {:?} ({})", test_name, expected_error, expected_code);
                } else {
                    panic!("Expected custom error code for {}, got: {:?}", test_name, error);
                }
            }
            _ => panic!("Unexpected program result for {}: {:?}", test_name, result.program_result),
        }
    }

    pub fn assert_instruction_failed(result: mollusk_svm::result::InstructionResult, test_name: &str) {
        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                panic!(
                    "Expected instruction to fail for {}, but it succeeded",
                    test_name
                );
            }
            _ => {
                println!(
                    "✅ {} correctly rejected: {:?}",
                    test_name, result.program_result
                );
            }
        }
    }

}
