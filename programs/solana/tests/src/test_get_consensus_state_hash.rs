use anchor_client::solana_sdk::pubkey::Pubkey;
use ics07_tendermint::{ClientState, ConsensusState};
use std::str::FromStr;

use crate::helpers::{generate_unique_chain_id, initialize_contract, log, setup_test_env};

#[ignore = "Needs to be fixed; We need real fixtures for this test"]
#[test]
fn test_get_consensus_state_hash() {
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

    // Get the consensus state store PDA for height 1 (initial height)
    let (consensus_state_store, _bump) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            contract.client_data_pda.as_ref(),
            &1u64.to_le_bytes(),
        ],
        &env.program.id(),
    );

    // Call get_consensus_state_hash
    let tx_result = env
        .program
        .request()
        .accounts(ics07_tendermint::accounts::GetConsensusStateHash {
            client_state: contract.client_data_pda,
            consensus_state_store,
        })
        .args(ics07_tendermint::instruction::GetConsensusStateHash { revision_height: 1 })
        .send();

    match tx_result {
        Ok(sig) => log(
            &env,
            &format!("✅ Get consensus state hash successful: {}", sig),
        ),
        Err(e) => panic!("❌ Failed to get consensus state hash: {}", e),
    }

    // Note: anchor-client doesn't support getting return data directly
    // In a real test, you would need to use the RPC client to fetch the transaction
    // and extract the return data from it
}
