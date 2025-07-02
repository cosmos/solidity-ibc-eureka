use crate::common::with_initialized_client;
use anchor_client::solana_sdk::signer::Signer;
use ics07_tendermint::MembershipMsg;

#[test]
fn test_verify_membership() {
    with_initialized_client("verify_membership", |program, client_data| {
        let membership_msg = MembershipMsg {
            height: 100,
            delay_time_period: 0,
            delay_block_period: 0,
            proof: vec![10, 20, 30], // Mock proof
            path: b"path/to/key".to_vec(),
            value: b"some_value".to_vec(),
        };

        let verify_result = program
            .request()
            .accounts(ics07_tendermint::accounts::VerifyMembership {
                client_data: client_data.pubkey(),
            })
            .args(ics07_tendermint::instruction::VerifyMembership {
                msg: membership_msg,
            })
            .send()?;

        println!("✅ Verify membership successful: {}", verify_result);
        Ok(())
    });
}
