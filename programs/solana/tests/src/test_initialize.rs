// use anchor_client::solana_sdk::{signer::Signer, system_program, pubkey::Pubkey};
// use crate::helpers::{setup_test_environment, create_client, load_program_or_fail, create_test_client_state, create_test_consensus_state};
use anchor_client::solana_sdk::pubkey::Pubkey;
use ics07_tendermint::{ClientState, ConsensusState};
use std::str::FromStr;

use crate::helpers::{generate_unique_chain_id, initialize_contract, log, setup_test_env};

// FIXME: make it work
#[ignore]
#[test]
fn test_initialize_with_pda() {
    let program_id = Pubkey::from_str("8wQAC7oWLTxExhR49jYAzXZB39mu7WVVvkWJGgAMMjpV").unwrap();

    let client_state = ClientState {
        chain_id: generate_unique_chain_id(),
        trust_level_numerator: 1,
        trust_level_denominator: 3,
        trusting_period: 1000,
        unbonding_period: 2000,
        max_clock_drift: 5,
        frozen_height: 0,
        latest_height: 42,
    };

    let consensus_state = ConsensusState {
        timestamp: 123456789,
        root: [0u8; 32],
        next_validators_hash: [1u8; 32],
    };

    let env = setup_test_env(program_id);
    let contract = initialize_contract(&env, program_id, client_state, consensus_state);

    let account = env
        .program
        .account::<ics07_tendermint::ClientData>(contract.client_data_pda)
        .expect("Failed to fetch client_data account");

    assert_eq!(
        account.client_state.chain_id,
        contract.client_state.chain_id
    );
    assert_eq!(account.client_state.latest_height, 42);
    assert_eq!(account.consensus_state.timestamp, 123456789);
    assert_eq!(account.frozen, false);

    log(&env, "✅ Test passed - contract initialized successfully");
}
