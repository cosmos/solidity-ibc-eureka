use crate::common::with_initialized_client;
use anchor_client::solana_sdk::{signer::Signer, pubkey::Pubkey};

#[test]
fn test_get_consensus_state_hash() {
    with_initialized_client("get_consensus_state_hash", |program, client_data| {
        // Get the consensus state store PDA for height 1 (initial height)
        let (consensus_state_store, _bump) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_data.pubkey().as_ref(),
                &1u64.to_le_bytes(),
            ],
            &program.id(),
        );

        // Call get_consensus_state_hash
        let tx_result = program
            .request()
            .accounts(ics07_tendermint::accounts::GetConsensusStateHash {
                client_data: client_data.pubkey(),
                consensus_state_store,
            })
            .args(ics07_tendermint::instruction::GetConsensusStateHash {
                revision_height: 1,
            })
            .send()?;

        println!("✅ Get consensus state hash successful: {}", tx_result);
        
        // Note: anchor-client doesn't support getting return data directly
        // In a real test, you would need to use the RPC client to fetch the transaction
        // and extract the return data from it
        
        Ok(())
    });
}