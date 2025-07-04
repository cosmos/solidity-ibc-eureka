use crate::common::with_initialized_client;
use anchor_client::solana_sdk::{signer::Signer, pubkey::Pubkey};
use ics07_tendermint::MembershipMsg;

#[test]
fn test_verify_non_membership() {
    with_initialized_client("verify_non_membership", |program, client_data| {
        // Use height 0 since that's what we have initialized
        let proof_height = 0u64;
        
        let non_membership_msg = MembershipMsg {
            height: proof_height,
            delay_time_period: 0,
            delay_block_period: 0,
            proof: vec![40, 50, 60], // Mock proof
            path: b"path/to/nonexistent".to_vec(),
            value: vec![], // Empty value for non-membership
        };

        // Get the consensus state store PDA for the proof height
        let (consensus_state_at_height, _bump) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_data.pubkey().as_ref(),
                &proof_height.to_le_bytes(),
            ],
            &program.id(),
        );

        let verify_non_result = program
            .request()
            .accounts(ics07_tendermint::accounts::VerifyNonMembership {
                client_data: client_data.pubkey(),
                consensus_state_at_height,
            })
            .args(ics07_tendermint::instruction::VerifyNonMembership {
                msg: non_membership_msg,
            })
            .send()?;

        println!("✅ Verify non-membership successful: {}", verify_non_result);
        Ok(())
    });
}
