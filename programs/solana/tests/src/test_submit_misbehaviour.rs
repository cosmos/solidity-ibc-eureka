use crate::common::with_initialized_client;
use anchor_client::solana_sdk::signer::Signer;
use ics07_tendermint::MisbehaviourMsg;

#[test]
fn test_submit_misbehaviour() {
    with_initialized_client("submit_misbehaviour", |program, client_data| {
        let misbehaviour_msg = MisbehaviourMsg {
            client_id: "test-client-id".to_string(),
            header_1: vec![100, 200], // Mock header 1
            header_2: vec![101, 201], // Mock header 2
        };

        let misbehaviour_result = program
            .request()
            .accounts(ics07_tendermint::accounts::SubmitMisbehaviour {
                client_data: client_data.pubkey(),
            })
            .args(ics07_tendermint::instruction::SubmitMisbehaviour {
                msg: misbehaviour_msg,
            })
            .send()?;

        println!("✅ Submit misbehaviour successful: {}", misbehaviour_result);
        Ok(())
    });
}
