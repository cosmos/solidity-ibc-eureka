use crate::common::with_initialized_client;
use anchor_client::solana_sdk::{signer::Signer, pubkey::Pubkey};

#[test]
fn test_get_consensus_state_hash() {
    with_initialized_client("get_consensus_state_hash", |program, client_data| {
        // Get the consensus state store PDA for height 0 (initial height)
        let (consensus_state_store, _bump) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_data.pubkey().as_ref(),
                &0u64.to_le_bytes(),
            ],
            &program.id(),
        );

        // Call get_consensus_state_hash
        let hash_result: [u8; 32] = program
            .request()
            .accounts(ics07_tendermint::accounts::GetConsensusStateHash {
                client_data: client_data.pubkey(),
                consensus_state_store,
            })
            .args(ics07_tendermint::instruction::GetConsensusStateHash {
                revision_height: 0,
            })
            .send_with_return_data()?;

        println!("✅ Get consensus state hash successful");
        println!("   Hash: 0x{}", hex::encode(hash_result));
        
        // Verify the hash is not zero
        assert_ne!(hash_result, [0u8; 32], "Hash should not be zero");
        
        Ok(())
    });
}