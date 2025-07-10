use anchor_client::solana_sdk::pubkey::Pubkey;
use ics07_tendermint::{ClientState, ConsensusState, MisbehaviourMsg};
use std::str::FromStr;

use crate::helpers::{
    create_test_misbehaviour_bytes, generate_unique_chain_id, initialize_contract, log,
    setup_test_env,
};

#[ignore = "Needs to be fixed; We need real fixtures for this test"]
#[test]
fn test_submit_misbehaviour() {
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

    let misbehaviour_msg = MisbehaviourMsg {
        client_id: "test-client-id".to_string(),
        misbehaviour: create_test_misbehaviour_bytes(),
    };

    // For testing, we'll use the same consensus state for both trusted states
    let (trusted_consensus_state_1, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            contract.client_data_pda.as_ref(),
            &1u64.to_le_bytes(),
        ],
        &env.program.id(),
    );
    let (trusted_consensus_state_2, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            contract.client_data_pda.as_ref(),
            &1u64.to_le_bytes(),
        ],
        &env.program.id(),
    );

    let misbehaviour_result = env
        .program
        .request()
        .accounts(ics07_tendermint::accounts::SubmitMisbehaviour {
            client_data: contract.client_data_pda,
            trusted_consensus_state_1,
            trusted_consensus_state_2,
        })
        .args(ics07_tendermint::instruction::SubmitMisbehaviour {
            msg: misbehaviour_msg,
        })
        .send();

    match misbehaviour_result {
        Ok(sig) => log(&env, &format!("✅ Submit misbehaviour successful: {}", sig)),
        Err(e) => panic!("❌ Failed to submit misbehaviour: {}", e),
    }
}
