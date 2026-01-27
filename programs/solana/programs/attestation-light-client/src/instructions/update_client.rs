use crate::error::ErrorCode;
use crate::helpers::{decode_state_attestation, deserialize_membership_proof, verify_attestation};
use crate::types::ConsensusState;
use crate::UpdateClient;
use anchor_lang::prelude::*;

/// Parameters for update_client instruction
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UpdateClientParams {
    pub proof: Vec<u8>,
}

// NOTE: Misbehavior Detection Difference from Solidity
//
// Solidity allows resubmitting to the same height:
// - Same timestamp → NoOp
// - Different timestamp → Freezes client, returns Misbehaviour
//
// Solana uses `init` constraint which fails if consensus state account exists:
// - First submission wins (immutable)
// - Subsequent submissions fail with "account already in use"
// - No misbehavior freezing mechanism
//
// This is a deliberate architectural choice. To match Solidity exactly would require:
// 1. Passing optional existing consensus state account
// 2. Using init_if_needed or manual account creation
// 3. Comparing timestamps and freezing on mismatch
//
// The current approach is simpler and prevents overwrites but doesn't report misbehavior.

pub fn update_client<'info>(
    ctx: Context<'_, '_, 'info, 'info, UpdateClient<'info>>,
    params: UpdateClientParams,
) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::PROOF_SUBMITTER_ROLE,
        &ctx.accounts.submitter,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    let client_state = &mut ctx.accounts.client_state;

    require!(!client_state.is_frozen, ErrorCode::ClientFrozen);

    let proof = deserialize_membership_proof(&params.proof)?;
    let attestation = decode_state_attestation(&proof.attestation_data)?;

    verify_attestation(client_state, &proof.attestation_data, &proof.signatures)?;

    // Validate height and timestamp are non-zero (matches Solidity behavior)
    require!(
        attestation.height > 0 && attestation.timestamp > 0,
        ErrorCode::InvalidState
    );

    // Update latest height if the new height is greater
    if attestation.height > client_state.latest_height {
        client_state.latest_height = attestation.height;
    }

    // Initialize the new consensus state
    let new_consensus_state_store = &mut ctx.accounts.new_consensus_state_store;
    new_consensus_state_store.height = attestation.height;
    new_consensus_state_store.consensus_state = ConsensusState {
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
        create_app_state_account, create_client_state_account, create_empty_account,
        create_instructions_sysvar_account, create_payer_account, create_system_program_account,
    };
    use crate::test_helpers::fixtures::{default_client_state, DEFAULT_CLIENT_ID};
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
        new_consensus_state_pda: Pubkey,
        app_state_pda: Pubkey,
        access_manager_pda: Pubkey,
        instructions_sysvar: Pubkey,
        submitter: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts(
        client_id: &str,
        new_height: u64,
        client_state: ClientState,
    ) -> TestAccounts {
        let client_state_pda = ClientState::pda(client_id);
        let new_consensus_state_pda = ConsensusStateStore::pda(&client_state_pda, new_height);
        let app_state_pda = AppState::pda();
        let submitter = Pubkey::new_unique();

        // Create access manager with submitter as relayer
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account(submitter, vec![submitter]);

        // Create instructions sysvar with proper pubkey and account data
        let (instructions_sysvar, instructions_sysvar_account) =
            create_instructions_sysvar_account();

        let accounts = vec![
            (client_state_pda, create_client_state_account(&client_state)),
            // app_state stores the access_manager PROGRAM ID, not the PDA
            (app_state_pda, create_app_state_account(access_manager::ID)),
            (access_manager_pda, access_manager_account),
            (instructions_sysvar, instructions_sysvar_account),
            (new_consensus_state_pda, create_empty_account()),
            (submitter, create_payer_account()),
            (system_program::ID, create_system_program_account()),
        ];

        TestAccounts {
            client_state_pda,
            new_consensus_state_pda,
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
        new_height: u64,
        params: UpdateClientParams,
    ) -> Instruction {
        let instruction_data = crate::instruction::UpdateClient {
            client_id: client_id.to_string(),
            new_height,
            params,
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(test_accounts.client_state_pda, false),
                AccountMeta::new_readonly(test_accounts.app_state_pda, false),
                AccountMeta::new_readonly(test_accounts.access_manager_pda, false),
                AccountMeta::new_readonly(test_accounts.instructions_sysvar, false),
                AccountMeta::new(test_accounts.new_consensus_state_pda, false),
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
            anchor_lang::error::Error::from(ErrorCode::ClientFrozen).into(),
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
            anchor_lang::error::Error::from(ErrorCode::NoSignatures).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_too_few_signatures() {
        let client_state = ClientState {
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
            anchor_lang::error::Error::from(ErrorCode::TooFewSignatures).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_update_client_duplicate_signatures() {
        let client_state = ClientState {
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![[1u8; 20], [2u8; 20]],
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(DEFAULT_CLIENT_ID, NEW_HEIGHT, client_state);

        let attestation_data =
            crate::test_helpers::fixtures::encode_state_attestation(NEW_HEIGHT, 1_700_000_000);

        let sig = vec![1u8; 65];
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
            anchor_lang::error::Error::from(ErrorCode::DuplicateSignature).into(),
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
            anchor_lang::error::Error::from(ErrorCode::TooFewSignatures).into(),
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
}
