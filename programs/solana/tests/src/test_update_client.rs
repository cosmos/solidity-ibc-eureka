use crate::common::with_initialized_client;
use anchor_client::solana_sdk::{signer::Signer, pubkey::Pubkey, system_program};
use ics07_tendermint::UpdateClientMsg;

#[test]
fn test_update_client() {
    with_initialized_client("update_client", |program, client_data| {
        let update_msg = UpdateClientMsg {
            client_message: vec![1, 2, 3, 4, 5], // Mock client message
        };

        // Get the client's current state to calculate the consensus state PDA
        let client_account = program.account::<ics07_tendermint::ClientData>(client_data.pubkey())?;
        let new_height = client_account.client_state.latest_height + 1; // Assuming next height
        
        // Calculate the consensus state store PDA for the new height
        let (consensus_state_store, _bump) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_data.pubkey().as_ref(),
                &new_height.to_le_bytes(),
            ],
            &program.id(),
        );

        let payer = program.payer();
        let update_result = program
            .request()
            .accounts(ics07_tendermint::accounts::UpdateClient {
                client_data: client_data.pubkey(),
                consensus_state_store,
                payer: payer.pubkey(),
                system_program: system_program::id(),
            })
            .args(ics07_tendermint::instruction::UpdateClient { msg: update_msg })
            .send()?;

        println!("✅ Update client successful: {}", update_result);
        Ok(())
    });
}
