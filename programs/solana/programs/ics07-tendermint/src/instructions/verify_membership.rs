use crate::error::ErrorCode;
use crate::helpers::{deserialize_merkle_proof, validate_proof_params};
use crate::VerifyMembership;
use anchor_lang::prelude::*;
use ics25_handler::MembershipMsg;
use tendermint_light_client_membership::KVPair;

pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
    require!(!msg.value.is_empty(), ErrorCode::MembershipEmptyValue);

    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    validate_proof_params(client_state, &msg)?;

    let proof = deserialize_merkle_proof(&msg.proof)?;
    let kv_pair = KVPair::new(msg.path.clone(), msg.value);
    let app_hash = consensus_state_store.consensus_state.root;

    tendermint_light_client_membership::membership(app_hash, &[(kv_pair, proof)])
        .map_err(|_| error!(ErrorCode::MembershipVerificationFailed))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::fixtures::*;
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use crate::types::{ClientState, ConsensusState, IbcHeight};
    use anchor_lang::solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };
    use anchor_lang::AccountSerialize;
    use anchor_lang::InstructionData;
    use ics25_handler::MembershipMsg;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;

    struct TestAccounts {
        client_state_pda: Pubkey,
        consensus_state_pda: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts(
        chain_id: String,
        height: u64,
        mut client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> TestAccounts {
        use crate::test_helpers::chunk_test_utils::{
            derive_client_state_pda, derive_consensus_state_pda,
        };

        let client_state_pda = derive_client_state_pda(&chain_id);
        let consensus_state_pda = derive_consensus_state_pda(&client_state_pda, height);

        // Ensure the height being verified is tracked in consensus_state_heights
        if !client_state.consensus_state_heights.contains(&height) {
            client_state.consensus_state_heights.push(height);
            client_state.consensus_state_heights.sort_unstable();
        }

        let mut client_data = vec![];
        client_state.try_serialize(&mut client_data).unwrap();

        let consensus_state_store = ConsensusStateStore {
            height,
            consensus_state,
            payer: Pubkey::new_unique(),
        };

        let mut consensus_data = vec![];
        consensus_state_store
            .try_serialize(&mut consensus_data)
            .unwrap();

        let accounts = vec![
            (
                client_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: client_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                consensus_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: consensus_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
        ];

        TestAccounts {
            client_state_pda,
            consensus_state_pda,
            accounts,
        }
    }

    fn create_membership_msg(fixture: &MembershipMsgFixture) -> MembershipMsg {
        let path: Vec<Vec<u8>> = fixture.path.iter().map(|s| s.as_bytes().to_vec()).collect();
        let proof = hex_to_bytes(&fixture.proof);
        let value = hex_to_bytes(&fixture.value);

        MembershipMsg {
            height: fixture.height,
            delay_time_period: fixture.delay_time_period,
            delay_block_period: fixture.delay_block_period,
            proof,
            path,
            value,
        }
    }

    fn create_verify_membership_instruction(
        test_accounts: &TestAccounts,
        msg: MembershipMsg,
    ) -> Instruction {
        let instruction_data = crate::instruction::VerifyMembership { msg };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(test_accounts.client_state_pda, false),
                AccountMeta::new_readonly(test_accounts.consensus_state_pda, false),
            ],
            data: instruction_data.data(),
        }
    }

    fn setup_membership_test() -> (MembershipVerificationFixture, TestAccounts, MembershipMsg) {
        let fixture = load_membership_verification_fixture("verify_membership_key_0");
        let client_state = decode_client_state_from_hex(&fixture.client_state_hex);
        let consensus_state = decode_consensus_state_from_hex(&fixture.consensus_state_hex);

        let test_accounts = setup_test_accounts(
            client_state.chain_id.clone(),
            fixture.membership_msg.height,
            client_state,
            consensus_state,
        );

        let msg = create_membership_msg(&fixture.membership_msg);

        (fixture, test_accounts, msg)
    }

    #[test]
    fn test_verify_membership_happy_path() {
        let (_fixture, test_accounts, msg) = setup_membership_test();

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_empty_value() {
        let (_fixture, test_accounts, mut msg) = setup_membership_test();
        msg.value = vec![];

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::MembershipEmptyValue).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_wrong_app_hash() {
        let fixture = load_membership_verification_fixture("verify_membership_key_0");
        let client_state = decode_client_state_from_hex(&fixture.client_state_hex);
        let mut consensus_state = decode_consensus_state_from_hex(&fixture.consensus_state_hex);

        consensus_state.root = [0xFF; 32];

        let test_accounts = setup_test_accounts(
            client_state.chain_id.clone(),
            fixture.membership_msg.height,
            client_state,
            consensus_state,
        );

        let msg = create_membership_msg(&fixture.membership_msg);
        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::MembershipVerificationFailed).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_tampered_value() {
        let (_fixture, test_accounts, mut msg) = setup_membership_test();
        msg.value.push(0xFF);

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::MembershipVerificationFailed).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_wrong_path() {
        let (_fixture, test_accounts, mut msg) = setup_membership_test();
        msg.path = vec![b"wrong".to_vec(), b"path".to_vec()];

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::MembershipVerificationFailed).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_invalid_proof() {
        let (_fixture, test_accounts, mut msg) = setup_membership_test();
        msg.proof = vec![0xFF; 100];

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidProof).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_frozen_client() {
        let fixture = load_membership_verification_fixture("verify_membership_key_0");
        let mut client_state = decode_client_state_from_hex(&fixture.client_state_hex);
        let consensus_state = decode_consensus_state_from_hex(&fixture.consensus_state_hex);

        client_state.frozen_height = IbcHeight {
            revision_number: 0,
            revision_height: 1,
        };

        let test_accounts = setup_test_accounts(
            client_state.chain_id.clone(),
            fixture.membership_msg.height,
            client_state,
            consensus_state,
        );

        let msg = create_membership_msg(&fixture.membership_msg);
        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::ClientFrozen).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_height_not_tracked() {
        use crate::test_helpers::chunk_test_utils::{
            derive_client_state_pda, derive_consensus_state_pda,
        };

        let fixture = load_membership_verification_fixture("verify_membership_key_0");
        let mut client_state = decode_client_state_from_hex(&fixture.client_state_hex);
        let consensus_state = decode_consensus_state_from_hex(&fixture.consensus_state_hex);

        // Set up client state with a different height in tracking list
        // This simulates the query height being pruned or never tracked
        client_state.consensus_state_heights = vec![fixture.membership_msg.height + 100];

        let client_state_pda = derive_client_state_pda(&client_state.chain_id);
        let consensus_state_pda =
            derive_consensus_state_pda(&client_state_pda, fixture.membership_msg.height);

        // Manually serialize client state WITHOUT adding the query height to tracking list
        let mut client_data = vec![];
        client_state.try_serialize(&mut client_data).unwrap();

        let consensus_state_store = ConsensusStateStore {
            height: fixture.membership_msg.height,
            consensus_state,
            payer: Pubkey::new_unique(),
        };

        let mut consensus_data = vec![];
        consensus_state_store
            .try_serialize(&mut consensus_data)
            .unwrap();

        let accounts = vec![
            (
                client_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: client_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                consensus_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: consensus_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
        ];

        let test_accounts = TestAccounts {
            client_state_pda,
            consensus_state_pda,
            accounts,
        };

        let msg = create_membership_msg(&fixture.membership_msg);
        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::ConsensusStateNotFound).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }
}
