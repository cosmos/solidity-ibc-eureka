use crate::abi_decode::decode_state_attestation;
use crate::error::ErrorCode;
use crate::events::MisbehaviourDetected;
use crate::proof::deserialize_membership_proof;
use crate::state::ConsensusStateStore;
use crate::types::{AppState, ClientState, ConsensusState};
use crate::verification::verify_attestation;
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(client_id: String, new_height: u64)]
pub struct UpdateClient<'info> {
    #[account(
        mut,
        constraint = client_state.client_id == client_id,
    )]
    pub client_state: Account<'info, ClientState>,

    #[account(
        seeds = [AppState::SEED],
        bump
    )]
    pub app_state: Account<'info, AppState>,

    /// CHECK: Validated by seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// CHECK: Instructions sysvar for role verification
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(
        init_if_needed,
        payer = submitter,
        space = 8 + ConsensusStateStore::INIT_SPACE,
        seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &new_height.to_le_bytes()],
        bump
    )]
    pub consensus_state_store: Account<'info, ConsensusStateStore>,

    #[account(mut)]
    pub submitter: Signer<'info>,

    pub system_program: Program<'info, System>,
}

/// Parameters for `update_client` instruction
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateClientParams {
    pub proof: Vec<u8>,
}

pub fn update_client<'info>(
    ctx: Context<'_, '_, 'info, 'info, UpdateClient<'info>>,
    params: UpdateClientParams,
) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::RELAYER_ROLE,
        &ctx.accounts.submitter,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let client_state = &mut ctx.accounts.client_state;

    require!(!client_state.is_frozen, ErrorCode::FrozenClientState);

    let proof = deserialize_membership_proof(&params.proof)?;
    let attestation = decode_state_attestation(&proof.attestation_data)?;

    verify_attestation(client_state, &proof.attestation_data, &proof.signatures)?;

    require!(
        attestation.height > 0 && attestation.timestamp > 0,
        ErrorCode::InvalidState
    );

    let consensus_state_store = &mut ctx.accounts.consensus_state_store;

    // Misbehaviour detection (matches Solidity behavior):
    // - If consensus state exists with same timestamp → NoOp (return early)
    // - If consensus state exists with different timestamp → freeze client and return success
    //   (must return Ok so state change persists - errors revert all changes in Solana)
    let existing_timestamp = consensus_state_store.consensus_state.timestamp;
    if existing_timestamp != 0 {
        if existing_timestamp != attestation.timestamp {
            client_state.is_frozen = true;
            emit!(MisbehaviourDetected {
                client_id: client_state.client_id.clone(),
                height: attestation.height,
                existing_timestamp,
                conflicting_timestamp: attestation.timestamp,
            });
        }
        return Ok(());
    }

    if attestation.height > client_state.latest_height {
        client_state.latest_height = attestation.height;
    }

    consensus_state_store.height = attestation.height;
    consensus_state_store.consensus_state = ConsensusState {
        height: attestation.height,
        timestamp: attestation.timestamp,
    };

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::access_control::create_access_manager_account;
    use crate::test_helpers::accounts::{
        create_app_state_account, create_client_state_account, create_consensus_state_account,
        create_empty_account, create_instructions_sysvar_account, create_payer_account,
        create_system_program_account,
    };
    use crate::test_helpers::fixtures::{default_client_state, DEFAULT_CLIENT_ID};
    use crate::test_helpers::signing::TestAttestor;
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use crate::types::{AppState, ClientState, MembershipProof};
    use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::system_program;

    const HEIGHT: u64 = 100;
    const NEW_HEIGHT: u64 = 200;

    struct TestAccounts {
        client_state_pda: Pubkey,
        consensus_state_pda: Pubkey,
        app_state_pda: Pubkey,
        access_manager_pda: Pubkey,
        instructions_sysvar: Pubkey,
        submitter: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts(
        client_id: &str,
        height: u64,
        client_state: ClientState,
    ) -> TestAccounts {
        setup_test_accounts_with_consensus(client_id, height, client_state, None)
    }

    fn setup_test_accounts_with_consensus(
        client_id: &str,
        height: u64,
        client_state: ClientState,
        existing_consensus_state: Option<ConsensusState>,
    ) -> TestAccounts {
        let client_state_pda = ClientState::pda(client_id);
        let consensus_state_pda = ConsensusStateStore::pda(&client_state_pda, height);
        let app_state_pda = AppState::pda();
        let submitter = Pubkey::new_unique();

        let (access_manager_pda, access_manager_account) =
            create_access_manager_account(submitter, vec![submitter]);

        let (instructions_sysvar, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let consensus_account = existing_consensus_state.map_or_else(create_empty_account, |cs| {
            create_consensus_state_account(height, cs)
        });

        let accounts = vec![
            (client_state_pda, create_client_state_account(&client_state)),
            (app_state_pda, create_app_state_account(access_manager::ID)),
            (access_manager_pda, access_manager_account),
            (instructions_sysvar, instructions_sysvar_account),
            (consensus_state_pda, consensus_account),
            (submitter, create_payer_account()),
            (system_program::ID, create_system_program_account()),
        ];

        TestAccounts {
            client_state_pda,
            consensus_state_pda,
            app_state_pda,
            access_manager_pda,
            instructions_sysvar,
            submitter,
            accounts,
        }
    }

    fn setup_default_test_accounts(client_id: &str, new_height: u64) -> TestAccounts {
        setup_test_accounts(
            client_id,
            new_height,
            default_client_state(client_id, HEIGHT),
        )
    }

    fn create_update_client_instruction(
        test_accounts: &TestAccounts,
        client_id: &str,
        height: u64,
        params: UpdateClientParams,
    ) -> Instruction {
        let instruction_data = crate::instruction::UpdateClient {
            client_id: client_id.to_string(),
            new_height: height,
            params,
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(test_accounts.client_state_pda, false),
                AccountMeta::new_readonly(test_accounts.app_state_pda, false),
                AccountMeta::new_readonly(test_accounts.access_manager_pda, false),
                AccountMeta::new_readonly(test_accounts.instructions_sysvar, false),
                AccountMeta::new(test_accounts.consensus_state_pda, false),
                AccountMeta::new(test_accounts.submitter, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    #[test]
    fn test_update_client_frozen() {
        let mut client_state = default_client_state(DEFAULT_CLIENT_ID, HEIGHT);
        client_state.is_frozen = true;

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data: vec![],
                signatures: vec![],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::FrozenClientState).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_invalid_proof() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT);

        let params = UpdateClientParams {
            proof: vec![0xFF; 100],
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        // Garbage data may cause program crash or specific error - both are acceptable
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail with invalid proof data"
        );
    }

    #[test]
    fn test_update_client_no_signatures() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_700_000_000);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::EmptySignatures).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_too_few_signatures() {
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![[1u8; 20], [2u8; 20]],
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_700_000_000);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![vec![0u8; 65]],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::ThresholdNotMet).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_duplicate_signers() {
        let attestor = TestAttestor::new(1);
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_700_000_000);

        // Same attestor signs twice - recovers to same address
        let sig = attestor.sign(&attestation_data);
        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![sig.clone(), sig],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::DuplicateSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_empty_proof_bytes() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT);

        let params = UpdateClientParams { proof: vec![] };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidProof).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_invalid_signature_length() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_700_000_000);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![vec![0u8; 64]],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidSignature).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_attestation_data_too_short() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data: vec![0u8; 64],
                signatures: vec![vec![0u8; 65]],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail for short attestation data"
        );
    }

    #[test]
    fn test_update_client_max_attestors() {
        let attestor_addresses: Vec<[u8; 20]> = (0..10).map(|i| [i as u8; 20]).collect();
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses,
            min_required_sigs: 7,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_700_000_000);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![
                    vec![1u8; 65],
                    vec![2u8; 65],
                    vec![3u8; 65],
                    vec![4u8; 65],
                    vec![5u8; 65],
                ],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::ThresholdNotMet).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_same_height() {
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            default_client_state(DEFAULT_CLIENT_ID, HEIGHT),
        );

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(HEIGHT, 1_700_000_000);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![vec![0u8; 65]],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_update_client_lower_height() {
        let lower_height = 50u64;
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            lower_height,
            default_client_state(DEFAULT_CLIENT_ID, HEIGHT),
        );

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(lower_height, 1_700_000_000);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![vec![0u8; 65]],
            })
            .unwrap(),
        };

        let instruction = create_update_client_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            lower_height,
            params,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_update_client_max_height() {
        let max_height = u64::MAX;
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            max_height,
            default_client_state(DEFAULT_CLIENT_ID, HEIGHT),
        );

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(max_height, 1_700_000_000);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![vec![0u8; 65]],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, max_height, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_update_client_misbehaviour_different_timestamp() {
        let existing_timestamp = 1_700_000_000u64;
        let conflicting_timestamp = 1_800_000_000u64;

        // Create test attestor with known keys
        let attestor = TestAttestor::new(1);

        let existing_consensus = ConsensusState {
            height: HEIGHT,
            timestamp: existing_timestamp,
        };

        // Create client state with the test attestor's address
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts_with_consensus(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            Some(existing_consensus),
        );

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(HEIGHT, conflicting_timestamp);

        // Sign the attestation data with the test attestor
        let signature = attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(
            &instruction,
            &test_accounts.accounts,
            &checks,
        );

        // Check that client state is frozen in the result accounts
        let client_state_account = result
            .get_account(&test_accounts.client_state_pda)
            .expect("Client state account should exist");

        let client_state: ClientState =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &client_state_account.data[..])
                .expect("Failed to deserialize client state");

        assert!(
            client_state.is_frozen,
            "Client state should be frozen after misbehaviour detection"
        );
    }

    #[test]
    fn test_update_client_same_height_same_timestamp_noop() {
        let timestamp = 1_700_000_000u64;

        // Create test attestor with known keys
        let attestor = TestAttestor::new(1);

        let existing_consensus = ConsensusState {
            height: HEIGHT,
            timestamp,
        };

        // Create client state with the test attestor's address
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts_with_consensus(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            Some(existing_consensus),
        );

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(HEIGHT, timestamp);

        // Sign the attestation data with the test attestor
        let signature = attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(
            &instruction,
            &test_accounts.accounts,
            &checks,
        );

        // Client should NOT be frozen
        let client_state_account = result
            .get_account(&test_accounts.client_state_pda)
            .expect("Client state account should exist");

        let client_state: ClientState =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &client_state_account.data[..])
                .expect("Failed to deserialize client state");

        assert!(
            !client_state.is_frozen,
            "Client state should NOT be frozen for same timestamp"
        );
    }

    #[test]
    fn test_update_client_happy_path_new_height() {
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let new_timestamp = 1_800_000_000u64;
        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, new_timestamp);

        let signature = attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(
            &instruction,
            &test_accounts.accounts,
            &checks,
        );

        // Verify client state updated latest_height
        let client_state_account = result
            .get_account(&test_accounts.client_state_pda)
            .expect("Client state account should exist");

        let client_state: ClientState =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &client_state_account.data[..])
                .expect("Failed to deserialize client state");

        assert_eq!(client_state.latest_height, NEW_HEIGHT);
        assert!(!client_state.is_frozen);

        // Verify consensus state was created
        let consensus_state_account = result
            .get_account(&test_accounts.consensus_state_pda)
            .expect("Consensus state account should exist");

        let consensus_state_store: crate::state::ConsensusStateStore =
            anchor_lang::AccountDeserialize::try_deserialize(
                &mut &consensus_state_account.data[..],
            )
            .expect("Failed to deserialize consensus state");

        assert_eq!(consensus_state_store.height, NEW_HEIGHT);
        assert_eq!(consensus_state_store.consensus_state.height, NEW_HEIGHT);
        assert_eq!(
            consensus_state_store.consensus_state.timestamp,
            new_timestamp
        );
    }

    #[test]
    fn test_update_client_invalid_state_zero_height() {
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        // Create attestation with zero height
        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(0, 1_700_000_000);

        let signature = attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidState).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_invalid_state_zero_timestamp() {
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        // Create attestation with zero timestamp
        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 0);

        let signature = attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidState).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_lower_height_does_not_update_latest() {
        let attestor = TestAttestor::new(1);

        // Client already at HEIGHT=100
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        // Update to a lower height (50 < 100)
        let lower_height = 50u64;
        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, lower_height, client_state);

        let new_timestamp = 1_600_000_000u64;
        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(lower_height, new_timestamp);

        let signature = attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction = create_update_client_instruction(
            &test_accounts,
            DEFAULT_CLIENT_ID,
            lower_height,
            params,
        );

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(
            &instruction,
            &test_accounts.accounts,
            &checks,
        );

        // Verify latest_height was NOT updated (still 100, not 50)
        let client_state_account = result
            .get_account(&test_accounts.client_state_pda)
            .expect("Client state account should exist");

        let client_state: ClientState =
            anchor_lang::AccountDeserialize::try_deserialize(&mut &client_state_account.data[..])
                .expect("Failed to deserialize client state");

        assert_eq!(
            client_state.latest_height, HEIGHT,
            "latest_height should remain unchanged"
        );

        // But consensus state should still be created
        let consensus_state_account = result
            .get_account(&test_accounts.consensus_state_pda)
            .expect("Consensus state account should exist");

        let consensus_state_store: crate::state::ConsensusStateStore =
            anchor_lang::AccountDeserialize::try_deserialize(
                &mut &consensus_state_account.data[..],
            )
            .expect("Failed to deserialize consensus state");

        assert_eq!(consensus_state_store.height, lower_height);
        assert_eq!(
            consensus_state_store.consensus_state.timestamp,
            new_timestamp
        );
    }

    #[test]
    fn test_update_client_multi_attestor_quorum() {
        use crate::test_helpers::signing::create_test_attestors;

        let attestors = create_test_attestors(3);
        let attestor_addresses: Vec<[u8; 20]> = attestors.iter().map(|a| a.eth_address).collect();

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses,
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let new_timestamp = 1_800_000_000u64;
        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, new_timestamp);

        // Sign with 2 out of 3 attestors
        let sig1 = attestors[0].sign(&attestation_data);
        let sig2 = attestors[2].sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![sig1, sig2],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_unknown_signer() {
        let trusted_attestor = TestAttestor::new(1);
        let untrusted_attestor = TestAttestor::new(99);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![trusted_attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_800_000_000);

        // Sign with untrusted attestor
        let signature = untrusted_attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::UnknownSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_more_signatures_than_required() {
        // 5 attestors, only 3 required, but all 5 sign - should succeed
        let attestors: Vec<_> = (1..=5).map(TestAttestor::new).collect();
        let addresses: Vec<_> = attestors.iter().map(|a| a.eth_address).collect();

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: addresses,
            min_required_sigs: 3,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_800_000_000);

        // All 5 attestors sign
        let signatures: Vec<_> = attestors
            .iter()
            .map(|a| a.sign(&attestation_data))
            .collect();

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures,
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_mixed_trusted_and_unknown_signer() {
        // First signer is trusted, second is unknown - should fail
        let trusted_attestor = TestAttestor::new(1);
        let unknown_attestor = TestAttestor::new(2);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![trusted_attestor.eth_address],
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_800_000_000);

        let sig1 = trusted_attestor.sign(&attestation_data);
        let sig2 = unknown_attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![sig1, sig2],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::UnknownSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_single_attestor_happy_path() {
        // Minimal quorum: 1 attestor, 1 required signature
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_800_000_000);

        let signature = attestor.sign(&attestation_data);

        let params = UpdateClientParams {
            proof: borsh::to_vec(&MembershipProof {
                attestation_data,
                signatures: vec![signature],
            })
            .unwrap(),
        };

        let instruction =
            create_update_client_instruction(&test_accounts, DEFAULT_CLIENT_ID, NEW_HEIGHT, params);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }
}
