use crate::common::with_initialized_client;
use anchor_client::solana_sdk::signer::Signer;
use ics07_tendermint::UpdateClientMsg;

#[test]
fn test_update_client() {
    with_initialized_client("update_client", |program, client_data| {
        let update_msg = UpdateClientMsg {
            client_message: vec![1, 2, 3, 4, 5], // Mock client message
        };

        let update_result = program
            .request()
            .accounts(ics07_tendermint::accounts::UpdateClient {
                client_data: client_data.pubkey(),
            })
            .args(ics07_tendermint::instruction::UpdateClient { msg: update_msg })
            .send()?;

        println!("✅ Update client successful: {}", update_result);
        Ok(())
    });
}
