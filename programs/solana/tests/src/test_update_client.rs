// use crate::helpers::with_initialized_client;
// use crate::test_helpers::create_test_header_bytes;
// use anchor_client::solana_sdk::{signer::Signer, pubkey::Pubkey, system_program};
// use ics07_tendermint::UpdateClientMsg;

// #[test]
// fn test_update_client() {
//     with_initialized_client("update_client", |program, client_data| {
//         let update_msg = UpdateClientMsg {
//             client_message: create_test_header_bytes(),
//         };

//         // Get the client's current state to calculate the consensus state PDA
//         let client_account = program.account::<ics07_tendermint::ClientData>(client_data.pubkey())?;
//         let new_height = client_account.client_state.latest_height + 1; // Assuming next height

//         // Calculate the consensus state store PDA for the new height
//         let (consensus_state_store, _bump) = Pubkey::find_program_address(
//             &[
//                 b"consensus_state",
//                 client_data.pubkey().as_ref(),
//                 &new_height.to_le_bytes(),
//             ],
//             &program.id(),
//         );

//         let update_result = program
//             .request()
//             .accounts(ics07_tendermint::accounts::UpdateClient {
//                 client_data: client_data.pubkey(),
//                 consensus_state_store,
//                 payer: program.payer(),
//                 system_program: system_program::id(),
//             })
//             .args(ics07_tendermint::instruction::UpdateClient { msg: update_msg })
//             .send()?;

//         println!("✅ Update client successful: {}", update_result);
//         Ok(())
//     });
// }
