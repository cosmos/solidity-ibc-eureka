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

    #[derive(Deserialize)]
    pub struct UpdateClientMessageFixture {
        pub client_message_hex: String,
        pub type_url: String,
        pub trusted_height: u64,
        pub new_height: u64,
    }

    pub fn load_client_state_fixture() -> ClientStateFixture {
        let fixture_str = include_str!("../../../tests/fixtures/client_state.json");
        serde_json::from_str(fixture_str).expect("Failed to parse client_state.json")
    }

    pub fn load_consensus_state_fixture() -> ConsensusStateFixture {
        let fixture_str = include_str!("../../../tests/fixtures/consensus_state.json");
        serde_json::from_str(fixture_str).expect("Failed to parse consensus_state.json")
    }

    pub fn load_update_client_message_fixture() -> UpdateClientMessageFixture {
        let fixture_str = include_str!("../../../tests/fixtures/update_client_message.json");
        serde_json::from_str(fixture_str).expect("Failed to parse update_client_message.json")
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
}