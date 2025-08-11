use crate::error::ErrorCode;
use crate::helpers::{deserialize_merkle_proof, validate_proof_params};
use crate::VerifyMembership;
use anchor_lang::prelude::*;
use solana_light_client_interface::MembershipMsg;
use tendermint_light_client_membership::KVPair;

pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
    require!(!msg.value.is_empty(), ErrorCode::MembershipEmptyValue);

    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    validate_proof_params(client_state, consensus_state_store, &msg)?;

    let proof = deserialize_merkle_proof(&msg.proof)?;

    let kv_pair = KVPair::new(msg.path, msg.value);
    let app_hash = consensus_state_store.consensus_state.root;

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)]).map_err(|e| {
        msg!("❌ Membership verification failed with error: {:?}", e);
        error!(ErrorCode::MembershipVerificationFailed)
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::fixtures::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    struct TestAccounts {
        client_state_pda: Pubkey,
        consensus_state_store_pda: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_initialized_client_for_membership() -> TestAccounts {
        // Use the same pattern as update_client: run Initialize instruction to get properly formatted accounts
        let fixture = load_membership_fixture_data();
        let client_state = client_state_from_fixture(&fixture.client_state);
        let consensus_state = consensus_state_from_fixture(&fixture.consensus_state);
        let target_height = fixture.membership_msg.height;

        let chain_id = &client_state.chain_id;
        let payer = Pubkey::new_unique();
        let latest_height = client_state.latest_height.revision_height;

        let (client_state_pda, _) =
            Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], &crate::ID);
        let (consensus_state_store_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state_pda.as_ref(),
                &latest_height.to_le_bytes(),
            ],
            &crate::ID,
        );

        // Create Initialize instruction
        let instruction_data = crate::instruction::Initialize {
            chain_id: chain_id.to_string(),
            latest_height,
            client_state: client_state.clone(),
            consensus_state: consensus_state.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(consensus_state_store_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Create empty accounts for Initialize instruction to populate
        let accounts = vec![
            (
                client_state_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                consensus_state_store_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payer,
                Account {
                    lamports: 10_000_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                system_program::ID,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
        ];

        // Execute Initialize instruction to get properly formatted accounts
        let mollusk = Mollusk::new(&crate::ID, "../../target/deploy/ics07_tendermint");
        let result = mollusk.process_instruction(&instruction, &accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                // We need to create a consensus state at the proof height to match the membership proof
                let (target_consensus_state_pda, _) = Pubkey::find_program_address(
                    &[
                        b"consensus_state",
                        client_state_pda.as_ref(),
                        &target_height.to_le_bytes(),
                    ],
                    &crate::ID,
                );

                // Create the consensus state at the target height manually using the same pattern as Initialize
                let mut accounts_with_target_height = result.resulting_accounts;

                // Create the consensus state store for the target height
                let consensus_state_store_target = ConsensusStateStore {
                    height: target_height,
                    consensus_state,
                };

                let mut consensus_state_data = vec![];
                consensus_state_store_target
                    .try_serialize(&mut consensus_state_data)
                    .unwrap();

                accounts_with_target_height.push((
                    target_consensus_state_pda,
                    Account {
                        lamports: 1_000_000_000,
                        data: consensus_state_data,
                        owner: crate::ID,
                        executable: false,
                        rent_epoch: 0,
                    },
                ));

                TestAccounts {
                    client_state_pda,
                    consensus_state_store_pda: target_consensus_state_pda,
                    accounts: accounts_with_target_height,
                }
            }
            _ => panic!("Initialize instruction failed: {:?}", result.program_result),
        }
    }

    fn create_verify_membership_instruction(
        test_accounts: &TestAccounts,
        msg: &MembershipMsg,
    ) -> Instruction {
        use crate::instruction;

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(test_accounts.client_state_pda, false),
                AccountMeta::new_readonly(test_accounts.consensus_state_store_pda, false),
            ],
            data: instruction::VerifyMembership { msg: msg.clone() }.data(),
        }
    }

    #[test]
    fn test_proof_data_standalone() {
        use ibc_core_commitment_types::merkle::MerkleProof;
        use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
        use ibc_proto::Protobuf;
        use prost::Message;

        // Test proof data parsing outside of Solana context
        let fixture = load_membership_fixture_data();
        let proof_hex = &fixture.membership_msg.proof;
        let proof_bytes = hex_to_bytes(proof_hex);

        println!("Proof hex: {proof_hex}");
        println!("Proof bytes length: {}", proof_bytes.len());
        println!(
            "First 32 bytes: {:?}",
            &proof_bytes[..proof_bytes.len().min(32)]
        );

        // The issue: This data is in ABCI ProofOps format but we need MerkleProof format
        // The first byte 0a (10) indicates field 1, which is the 'ops' field in ProofOps
        println!(
            "First byte analysis: 0x{:02x} = field {} wire type {}",
            proof_bytes[0],
            proof_bytes[0] >> 3,
            proof_bytes[0] & 0x07
        );

        // This confirms the data is ABCI ProofOps, not IBC MerkleProof
        // We need to either:
        // 1. Update the fixture generation to output MerkleProof format, or
        // 2. Update our deserializer to handle ProofOps format

        // For now, let's document this finding
        println!(
            "🔍 ANALYSIS: The proof data is in ABCI ProofOps format, not IBC MerkleProof format"
        );
        println!("   This explains the 'unexpected end group tag' error when trying to decode as MerkleProof");

        // Try to parse the proof directly using ibc-proto (this will fail as expected)
        match <RawMerkleProof as Message>::decode(&proof_bytes[..]) {
            Ok(raw_proof) => {
                println!("✅ Raw protobuf decode successful: {raw_proof:?}");
            }
            Err(e) => {
                println!("❌ Raw protobuf decode failed (expected): {e:?}");
            }
        }

        // Try using ibc-rs deserializer (this will also fail as expected)
        match <MerkleProof as Protobuf<RawMerkleProof>>::decode_vec(&proof_bytes) {
            Ok(proof) => {
                println!("✅ IBC-rs decode successful: {proof:?}");
            }
            Err(e) => {
                println!("❌ IBC-rs decode failed (expected): {e:?}");
            }
        }
    }

    #[test]
    fn test_verify_membership_happy_path() {
        let fixture = load_membership_fixture_data();

        // Convert fixture membership msg to actual MembershipMsg
        let membership_msg = MembershipMsg {
            delay_block_period: fixture.membership_msg.delay_block_period,
            delay_time_period: fixture.membership_msg.delay_time_period,
            height: fixture.membership_msg.height,
            path: fixture
                .membership_msg
                .path
                .iter()
                .map(|s| s.as_bytes().to_vec())
                .collect(),
            proof: hex_to_bytes(&fixture.membership_msg.proof),
            value: hex_to_bytes(&fixture.membership_msg.value),
        };

        let test_accounts = setup_initialized_client_for_membership();

        let instruction = create_verify_membership_instruction(&test_accounts, &membership_msg);

        let mollusk = Mollusk::new(&crate::ID, "../../target/deploy/ics07_tendermint");

        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!("✅ Membership verification successful for predefined key");
            }
            mollusk_svm::result::ProgramResult::Failure(error) => {
                panic!("❌ Membership verification failed with error: {error:?}");
            }
            mollusk_svm::result::ProgramResult::UnknownError(error) => {
                panic!("❌ Membership verification failed with unknown error: {error:?}");
            }
        }
    }
}
