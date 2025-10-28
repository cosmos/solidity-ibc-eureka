use std::sync::LazyLock;

use mollusk_svm::result::Check;

pub const PROGRAM_BINARY_PATH: &str = "../../target/deploy/ics07_tendermint";

pub static SUCCESS_CHECK: LazyLock<Vec<Check>> = LazyLock::new(|| vec![Check::success()]);

pub mod fixtures {
    use crate::types::{ClientState, ConsensusState, IbcHeight};
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct UpdateClientFixture {
        pub client_state_hex: String,
        pub consensus_state_hex: String,
        pub update_client_message: UpdateClientMessage,
    }

    #[derive(Deserialize)]
    pub struct UpdateClientMessage {
        pub client_message_hex: String,
        pub trusted_height: u64,
        pub new_height: u64,
    }

    fn load_fixture(filename: &str) -> UpdateClientFixture {
        let fixture_path =
            format!("../../../../packages/tendermint-light-client/fixtures/{filename}.json");
        let fixture_content = std::fs::read_to_string(&fixture_path)
            .unwrap_or_else(|_| panic!("Failed to read fixture: {fixture_path}"));

        serde_json::from_str(&fixture_content)
            .unwrap_or_else(|_| panic!("Failed to parse fixture: {fixture_path}"))
    }

    pub fn load_update_client_message(filename: &str) -> UpdateClientMessage {
        let fixture = load_fixture(filename);
        fixture.update_client_message
    }

    pub fn load_primary_fixtures() -> (ClientState, ConsensusState, UpdateClientMessage) {
        let fixture = load_fixture("update_client_happy_path");
        (
            decode_client_state(&fixture.client_state_hex),
            decode_consensus_state(&fixture.consensus_state_hex),
            fixture.update_client_message,
        )
    }

    fn decode_client_state(client_state_hex: &str) -> ClientState {
        use prost::Message;

        let bytes = hex_to_bytes(client_state_hex);
        let proto = ibc_client_tendermint::types::proto::v1::ClientState::decode(&bytes[..])
            .expect("Failed to decode client state");

        let trust_level = proto
            .trust_level
            .expect("Missing trust_level in client state");
        let trusting_period = proto
            .trusting_period
            .expect("Missing trusting_period in client state");
        let unbonding_period = proto
            .unbonding_period
            .expect("Missing unbonding_period in client state");
        let max_clock_drift = proto
            .max_clock_drift
            .expect("Missing max_clock_drift in client state");
        let latest_height = proto
            .latest_height
            .expect("Missing latest_height in client state");

        ClientState {
            chain_id: proto.chain_id,
            trust_level_numerator: trust_level.numerator as u64,
            trust_level_denominator: trust_level.denominator as u64,
            trusting_period: trusting_period.seconds as u64,
            unbonding_period: unbonding_period.seconds as u64,
            max_clock_drift: max_clock_drift.seconds as u64,
            frozen_height: proto
                .frozen_height
                .map_or_else(IbcHeight::default, |frozen_height| IbcHeight {
                    revision_number: frozen_height.revision_number,
                    revision_height: frozen_height.revision_height,
                }),
            latest_height: IbcHeight {
                revision_number: latest_height.revision_number,
                revision_height: latest_height.revision_height,
            },
            // Initialize with the latest height in the tracking list
            consensus_state_heights: vec![latest_height.revision_height],
        }
    }

    fn decode_consensus_state(consensus_state_hex: &str) -> ConsensusState {
        use prost::Message;

        let bytes = hex_to_bytes(consensus_state_hex);
        let proto = ibc_client_tendermint::types::proto::v1::ConsensusState::decode(&bytes[..])
            .expect("Failed to decode consensus state");

        let timestamp = proto
            .timestamp
            .expect("Missing timestamp in consensus state");
        let root = proto.root.expect("Missing root in consensus state");

        ConsensusState {
            timestamp: timestamp_to_nanoseconds(timestamp.seconds, timestamp.nanos),
            root: root
                .hash
                .as_slice()
                .try_into()
                .expect("Invalid root hash length"),
            next_validators_hash: proto
                .next_validators_hash
                .as_slice()
                .try_into()
                .expect("Invalid next_validators_hash length"),
        }
    }

    // Helper to check if a fixture file exists
    pub fn fixture_exists(filename: &str) -> bool {
        let fixture_path =
            format!("../../../../packages/tendermint-light-client/fixtures/{filename}.json");
        std::path::Path::new(&fixture_path).exists()
    }

    pub fn hex_to_bytes(hex_str: &str) -> Vec<u8> {
        let hex_str = hex_str.trim_start_matches("0x");
        hex::decode(hex_str).expect("Invalid hex string")
    }

    /// Convert protobuf Timestamp to nanoseconds since Unix epoch
    pub fn timestamp_to_nanoseconds(seconds: i64, nanos: i32) -> u64 {
        const NANOS_PER_SECOND: u64 = 1_000_000_000;
        (seconds as u64)
            .saturating_mul(NANOS_PER_SECOND)
            .saturating_add(nanos as u64)
    }

    /// Extract header timestamp from update client message
    /// Returns the header time as Unix timestamp in seconds (suitable for Clock sysvar)
    pub fn get_header_timestamp_from_message(message: &UpdateClientMessage) -> i64 {
        use crate::helpers::deserialize_header;

        let client_message = hex_to_bytes(&message.client_message_hex);
        let header =
            deserialize_header(&client_message).expect("Failed to deserialize header from fixture");

        // Extract timestamp from header and convert to Unix seconds
        let header_time_nanos = header.signed_header.header.time.unix_timestamp_nanos() as u64;
        (header_time_nanos / 1_000_000_000) as i64
    }

    /// Create a clock timestamp that's valid for the given header
    /// This adds a small buffer to the header time to pass clock drift validation
    pub fn get_valid_clock_timestamp_for_header(message: &UpdateClientMessage) -> i64 {
        let header_timestamp = get_header_timestamp_from_message(message);
        // Add 5 seconds buffer after header time to pass validation
        header_timestamp.saturating_add(5)
    }

    /// Create a clock timestamp that's way in the future to simulate expired header
    /// This is used for testing header expiration scenarios
    pub fn get_expired_clock_timestamp_for_header(message: &UpdateClientMessage) -> i64 {
        let header_timestamp = get_header_timestamp_from_message(message);
        // Add 1 year to make the header appear expired
        let one_year_in_seconds: i64 = 86400 * 365;
        header_timestamp.saturating_add(one_year_in_seconds)
    }

    /// Corrupt the header signature in the client message bytes
    /// Returns the corrupted bytes
    pub fn corrupt_header_signature(client_message_hex: &str) -> Vec<u8> {
        use prost::Message;

        let bytes = hex_to_bytes(client_message_hex);
        let mut header = ibc_client_tendermint::types::proto::v1::Header::decode(&bytes[..])
            .expect("Failed to decode header");

        // Corrupt the first signature we find
        if let Some(signed_header) = &mut header.signed_header {
            if let Some(commit) = &mut signed_header.commit {
                for sig in &mut commit.signatures {
                    if !sig.signature.is_empty() {
                        // Flip a bit in the middle of the signature
                        let mid_pos = sig.signature.len() / 2;
                        sig.signature[mid_pos] ^= 0x01;
                        break; // Only corrupt the first signature found
                    }
                }
            }
        }

        // Re-encode the corrupted header
        let mut buf = Vec::new();
        header.encode(&mut buf).expect("Failed to encode header");
        buf
    }

    /// Create client message bytes with wrong trusted height
    /// This modifies the `trusted_height` field in the protobuf
    pub fn create_message_with_wrong_trusted_height(
        client_message_hex: &str,
        wrong_height: u64,
    ) -> Vec<u8> {
        use prost::Message;

        let bytes = hex_to_bytes(client_message_hex);

        // Decode the header
        let mut header = ibc_client_tendermint::types::proto::v1::Header::decode(&bytes[..])
            .expect("Failed to decode header from test fixture");

        // Update the trusted height
        header.trusted_height = Some(ibc_proto::ibc::core::client::v1::Height {
            revision_number: 0,
            revision_height: wrong_height,
        });

        // Re-encode
        let mut buf = Vec::new();
        header.encode(&mut buf).expect("Failed to encode header");
        buf
    }

    // Generic test helper functions
    pub fn get_error_code(error: &anchor_lang::prelude::ProgramError) -> Option<u32> {
        match error {
            anchor_lang::prelude::ProgramError::Custom(code) => Some(*code),
            _ => None,
        }
    }

    pub fn assert_error_code(
        result: mollusk_svm::result::InstructionResult,
        expected_error: crate::error::ErrorCode,
        test_name: &str,
    ) {
        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                panic!("Expected {test_name} to fail with {expected_error:?}, but it succeeded");
            }
            mollusk_svm::result::ProgramResult::Failure(error) => {
                if let Some(code) = get_error_code(&error) {
                    let expected_code = expected_error as u32 + 6000; // Anchor errors start at 6000
                    assert_eq!(
                        code, expected_code,
                        "Expected {expected_error:?} ({expected_code}), but got error code {code}"
                    );
                    println!(
                        "✅ {test_name} correctly failed with {expected_error:?} ({expected_code})"
                    );
                } else {
                    panic!("Expected custom error code for {test_name}, got: {error:?}");
                }
            }
            mollusk_svm::result::ProgramResult::UnknownError(_) => panic!(
                "Unexpected program result for {}: {:?}",
                test_name, result.program_result
            ),
        }
    }

    pub fn assert_instruction_failed(
        result: mollusk_svm::result::InstructionResult,
        test_name: &str,
    ) {
        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                panic!("Expected instruction to fail for {test_name}, but it succeeded");
            }
            _ => {
                println!(
                    "✅ {} correctly rejected: {:?}",
                    test_name, result.program_result
                );
            }
        }
    }

    /// Membership verification fixture structures
    #[derive(Debug, serde::Deserialize)]
    pub struct MembershipMsgFixture {
        pub path: Vec<String>,
        pub proof: String,
        pub value: String,
        pub height: u64,
        pub delay_time_period: u64,
        pub delay_block_period: u64,
    }

    #[derive(Debug, serde::Deserialize)]
    pub struct MembershipVerificationFixture {
        pub membership_msg: MembershipMsgFixture,
        pub consensus_state_hex: String,
        pub client_state_hex: String,
    }

    pub fn load_membership_verification_fixture(filename: &str) -> MembershipVerificationFixture {
        let fixture_path =
            format!("../../../../packages/tendermint-light-client/fixtures/{filename}.json");
        let fixture_content = std::fs::read_to_string(&fixture_path)
            .unwrap_or_else(|_| panic!("Failed to read fixture: {fixture_path}"));

        serde_json::from_str(&fixture_content)
            .unwrap_or_else(|_| panic!("Failed to parse fixture: {fixture_path}"))
    }

    pub fn decode_client_state_from_hex(client_state_hex: &str) -> ClientState {
        use prost::Message;

        let bytes = hex_to_bytes(client_state_hex);
        let proto = ibc_client_tendermint::types::proto::v1::ClientState::decode(&bytes[..])
            .expect("Failed to decode client state");

        let trust_level = proto
            .trust_level
            .expect("Missing trust_level in client state");
        let trusting_period = proto
            .trusting_period
            .expect("Missing trusting_period in client state");
        let unbonding_period = proto
            .unbonding_period
            .expect("Missing unbonding_period in client state");
        let max_clock_drift = proto
            .max_clock_drift
            .expect("Missing max_clock_drift in client state");
        let latest_height = proto
            .latest_height
            .expect("Missing latest_height in client state");

        ClientState {
            chain_id: proto.chain_id,
            trust_level_numerator: trust_level.numerator as u64,
            trust_level_denominator: trust_level.denominator as u64,
            trusting_period: trusting_period.seconds as u64,
            unbonding_period: unbonding_period.seconds as u64,
            max_clock_drift: max_clock_drift.seconds as u64,
            frozen_height: proto
                .frozen_height
                .map_or_else(IbcHeight::default, |frozen_height| IbcHeight {
                    revision_number: frozen_height.revision_number,
                    revision_height: frozen_height.revision_height,
                }),
            latest_height: IbcHeight {
                revision_number: latest_height.revision_number,
                revision_height: latest_height.revision_height,
            },
        }
    }

    pub fn decode_consensus_state_from_hex(consensus_state_hex: &str) -> ConsensusState {
        use prost::Message;

        let bytes = hex_to_bytes(consensus_state_hex);
        let proto = ibc_client_tendermint::types::proto::v1::ConsensusState::decode(&bytes[..])
            .expect("Failed to decode consensus state");

        let timestamp = proto
            .timestamp
            .expect("Missing timestamp in consensus state");
        let root = proto.root.expect("Missing root in consensus state");

        let timestamp_nanos = timestamp_to_nanoseconds(timestamp.seconds, timestamp.nanos);

        ConsensusState {
            timestamp: timestamp_nanos,
            root: root
                .hash
                .as_slice()
                .try_into()
                .expect("Invalid root hash length"),
            next_validators_hash: proto
                .next_validators_hash
                .as_slice()
                .try_into()
                .expect("Invalid next_validators_hash length"),
        }
    }

    /// Helper functions for misbehaviour testing
    pub mod misbehaviour {
        use super::*;

        pub fn create_mock_tendermint_misbehaviour(
            _chain_id: &str,
            _header1_height: u64,
            _header2_height: u64,
            _trusted_height_1: u64,
            _trusted_height_2: u64,
            _conflicting_app_hashes: bool,
        ) -> Vec<u8> {
            vec![0xDE, 0xAD, 0xBE, 0xEF] // Mock data
        }

        pub fn misbehaviour_fixture_exists(filename: &str) -> bool {
            fixture_exists(filename)
        }

        pub fn load_misbehaviour_fixture(_filename: &str) -> Vec<u8> {
            vec![0xDE, 0xAD, 0xBE, 0xEF]
        }
    }
}

#[cfg(test)]
pub mod chunk_test_utils {
    use crate::state::{HeaderChunk, CHUNK_DATA_SIZE};
    use crate::types::{ClientState, ConsensusState, IbcHeight, UploadChunkParams};
    use anchor_lang::solana_program::keccak;
    use solana_sdk::account::Account;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    pub struct ChunkTestData {
        pub chunk_data: Vec<u8>,
        pub chunk_hash: [u8; 32],
    }

    pub fn create_test_chunk_data(index: u8, size: usize) -> ChunkTestData {
        let chunk_data = vec![index + 1; size];
        let chunk_hash = keccak::hash(&chunk_data).0;
        ChunkTestData {
            chunk_data,
            chunk_hash,
        }
    }

    pub fn create_chunk_account(chunk_data: Vec<u8>) -> Account {
        use anchor_lang::AccountSerialize;

        let chunk = HeaderChunk { chunk_data };

        let mut data = vec![];
        chunk.try_serialize(&mut data).unwrap();

        Account {
            lamports: 1_500_000, // Rent
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_client_state_account(chain_id: &str, latest_height: u64) -> Account {
        use anchor_lang::AccountSerialize;

        let client_state = ClientState {
            chain_id: chain_id.to_string(),
            trust_level_numerator: 2,
            trust_level_denominator: 3,
            trusting_period: 86400,
            unbonding_period: 172_800,
            max_clock_drift: 600,
            frozen_height: IbcHeight {
                revision_number: 0,
                revision_height: 0,
            },
            latest_height: IbcHeight {
                revision_number: 0,
                revision_height: latest_height,
            },
            // Initialize with the latest height in the tracking list
            consensus_state_heights: vec![latest_height],
        };

        let mut data = vec![];
        client_state.try_serialize(&mut data).unwrap();

        Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_consensus_state_account(
        root: [u8; 32],
        next_validators_hash: [u8; 32],
        timestamp: u64,
    ) -> Account {
        use crate::state::ConsensusStateStore;
        use anchor_lang::AccountSerialize;

        let consensus_state_store = ConsensusStateStore {
            height: 0, // Will be set by the actual instruction
            consensus_state: ConsensusState {
                timestamp,
                root,
                next_validators_hash,
            },
        };

        let mut data = vec![];
        consensus_state_store.try_serialize(&mut data).unwrap();

        Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_submitter_account(lamports: u64) -> Account {
        Account {
            lamports,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_upload_chunk_params(
        chain_id: &str,
        target_height: u64,
        chunk_index: u8,
        chunk_data: Vec<u8>,
    ) -> UploadChunkParams {
        UploadChunkParams {
            chain_id: chain_id.to_string(),
            target_height,
            chunk_index,
            chunk_data,
        }
    }

    pub fn derive_chunk_pda(
        submitter: &Pubkey,
        chain_id: &str,
        target_height: u64,
        chunk_index: u8,
    ) -> Pubkey {
        Pubkey::find_program_address(
            &[
                crate::state::HeaderChunk::SEED,
                submitter.as_ref(),
                chain_id.as_bytes(),
                &target_height.to_le_bytes(),
                &[chunk_index],
            ],
            &crate::ID,
        )
        .0
    }

    pub fn derive_client_state_pda(chain_id: &str) -> Pubkey {
        Pubkey::find_program_address(
            &[crate::types::ClientState::SEED, chain_id.as_bytes()],
            &crate::ID,
        )
        .0
    }

    pub fn derive_consensus_state_pda(client_state_key: &Pubkey, height: u64) -> Pubkey {
        Pubkey::find_program_address(
            &[
                crate::state::ConsensusStateStore::SEED,
                client_state_key.as_ref(),
                &height.to_le_bytes(),
            ],
            &crate::ID,
        )
        .0
    }

    pub fn create_valid_header_chunks(num_chunks: u8) -> (Vec<Vec<u8>>, [u8; 32]) {
        // Create realistic header data that can be reassembled
        let mut all_chunks = vec![];
        let mut full_header = vec![];

        for i in 0..num_chunks {
            let chunk_data = vec![i + 1; CHUNK_DATA_SIZE / 2]; // Half size for testing
            all_chunks.push(chunk_data.clone());
            full_header.extend(&chunk_data);
        }

        let header_commitment = keccak::hash(&full_header).0;

        (all_chunks, header_commitment)
    }
}
