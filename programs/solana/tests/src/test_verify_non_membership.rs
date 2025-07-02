use crate::common::with_initialized_client;
use anchor_client::solana_sdk::signer::Signer;
use ics07_tendermint::MembershipMsg;

#[test]
fn test_verify_non_membership() {
    with_initialized_client("verify_non_membership", |program, client_data| {
        let non_membership_msg = MembershipMsg {
            height: 101,
            delay_time_period: 0,
            delay_block_period: 0,
            proof: vec![40, 50, 60], // Mock proof
            path: b"path/to/nonexistent".to_vec(),
            value: vec![], // Empty value for non-membership
        };

        let verify_non_result = program
            .request()
            .accounts(ics07_tendermint::accounts::VerifyNonMembership {
                client_data: client_data.pubkey(),
            })
            .args(ics07_tendermint::instruction::VerifyNonMembership {
                msg: non_membership_msg,
            })
            .send()?;

        println!("✅ Verify non-membership successful: {}", verify_non_result);
        Ok(())
    });
}
