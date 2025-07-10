use anchor_client::solana_sdk::pubkey::Pubkey;
use anchor_client::solana_sdk::signer::Signer;
use ics07_tendermint::{ClientState, ConsensusState, UpdateClientMsg};
use std::str::FromStr;

use crate::helpers::{
    create_test_header_bytes, generate_unique_chain_id, initialize_contract, log, setup_test_env,
};

#[test]
fn test_update_client() {
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

    let update_msg = UpdateClientMsg {
        client_message: create_test_header_bytes(),
    };

    // Get the client's current state to calculate the consensus state PDA
    let client_account = env
        .program
        .account::<ics07_tendermint::ClientData>(contract.client_data_pda)
        .expect("Failed to fetch client_data account");
    let new_height = client_account.client_state.latest_height + 1; // Assuming next height

    // Calculate the consensus state store PDA for the new height
    let (consensus_state_store, _bump) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            contract.client_data_pda.as_ref(),
            &new_height.to_le_bytes(),
        ],
        &env.program.id(),
    );

    let update_result = env
        .program
        .request()
        .accounts(ics07_tendermint::accounts::UpdateClient {
            client_data: contract.client_data_pda,
            consensus_state_store,
            payer: env.payer.pubkey(),
            system_program: solana_system_interface::program::ID,
        })
        .args(ics07_tendermint::instruction::UpdateClient { msg: update_msg })
        .send();

    match update_result {
        Ok(sig) => log(&env, &format!("✅ Update client successful: {}", sig)),
        Err(e) => panic!("❌ Failed to update client: {}", e),
    }
}
