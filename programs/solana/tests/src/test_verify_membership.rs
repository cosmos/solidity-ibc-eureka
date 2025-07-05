use crate::common::with_initialized_client;
use crate::test_helpers::create_test_merkle_proof_bytes;
use anchor_client::solana_sdk::{signer::Signer, pubkey::Pubkey};
use ics07_tendermint::MembershipMsg;

#[test]
fn test_verify_membership() {
    with_initialized_client("verify_membership", |program, client_data| {
        // Use height 1 since that's what we initialized with
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
                client_data.pubkey().as_ref(),
                &proof_height.to_le_bytes(),
            ],
            &program.id(),
        );

        let verify_result = program
            .request()
            .accounts(ics07_tendermint::accounts::VerifyMembership {
                client_data: client_data.pubkey(),
                consensus_state_at_height,
            })
            .args(ics07_tendermint::instruction::VerifyMembership {
                msg: membership_msg,
            })
            .send()?;

        println!("✅ Verify membership successful: {}", verify_result);
        Ok(())
    });
}
