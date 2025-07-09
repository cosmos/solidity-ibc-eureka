use crate::common::with_initialized_client;
use crate::test_helpers::create_test_misbehaviour_bytes;
use anchor_client::solana_sdk::{pubkey::Pubkey, signer::Signer};
use ics07_tendermint::MisbehaviourMsg;

// FIXME: make it work
#[ignore]
#[test]
fn test_submit_misbehaviour() {
    with_initialized_client("submit_misbehaviour", |program, client_data| {
        // Create a properly encoded misbehaviour protobuf message
        let misbehaviour_msg = MisbehaviourMsg {
            client_id: "test-client-id".to_string(),
            misbehaviour: create_test_misbehaviour_bytes(),
        };

        // Get the client's current state
        let client_account =
            program.account::<ics07_tendermint::ClientData>(client_data.pubkey())?;
        let _current_height = client_account.client_state.latest_height;

        // For testing, we'll use the same consensus state for both trusted states
        // In a real scenario, these would be different heights from the misbehaviour headers
        let (trusted_consensus_state_1, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_data.pubkey().as_ref(),
                &1u64.to_le_bytes(), // Initial height from our test setup
            ],
            &program.id(),
        );

        let (trusted_consensus_state_2, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_data.pubkey().as_ref(),
                &1u64.to_le_bytes(), // Same height for testing
            ],
            &program.id(),
        );

        let misbehaviour_result = program
            .request()
            .accounts(ics07_tendermint::accounts::SubmitMisbehaviour {
                client_data: client_data.pubkey(),
                trusted_consensus_state_1,
                trusted_consensus_state_2,
            })
            .args(ics07_tendermint::instruction::SubmitMisbehaviour {
                msg: misbehaviour_msg,
            })
            .send()?;

        println!("✅ Submit misbehaviour successful: {}", misbehaviour_result);
        Ok(())
    });
}
