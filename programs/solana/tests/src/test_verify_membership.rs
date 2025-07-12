use anchor_client::solana_sdk::pubkey::Pubkey;
use ics07_tendermint::{ClientState, ConsensusState, MembershipMsg};
use std::str::FromStr;

use crate::helpers::{
    create_test_merkle_proof_bytes, generate_unique_chain_id, initialize_contract, log,
    setup_test_env,
};

#[ignore = "Needs to be fixed; We need real fixtures for this test"]
#[test]
fn test_verify_membership() {
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

    let proof_height = 1u64;
    let membership_msg = MembershipMsg {
        height: proof_height,
        delay_time_period: 0,
        delay_block_period: 0,
        proof: create_test_merkle_proof_bytes(),
        path: b"path/to/key".to_vec(),
        value: b"some_value".to_vec(),
    };

    // Get the consensus state store PDA for the proof height
    let (consensus_state_at_height, _bump) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            contract.client_data_pda.as_ref(),
            &proof_height.to_le_bytes(),
        ],
        &env.program.id(),
    );

    let verify_result = env
        .program
        .request()
        .accounts(ics07_tendermint::accounts::VerifyMembership {
            client_state: contract.client_data_pda,
            consensus_state_at_height,
        })
        .args(ics07_tendermint::instruction::VerifyMembership {
            msg: membership_msg,
        })
        .send();

    match verify_result {
        Ok(sig) => log(&env, &format!("✅ Verify membership successful: {}", sig)),
        Err(e) => panic!("❌ Failed to verify membership: {}", e),
    }
}
