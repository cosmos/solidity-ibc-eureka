use crate::error::ErrorCode;
use crate::types::{ClientState, ConsensusState};
use crate::Initialize;
use anchor_lang::prelude::*;

pub fn initialize(
    ctx: Context<Initialize>,
    client_state: ClientState,
    consensus_state: ConsensusState,
) -> Result<()> {
    require!(!client_state.chain_id.is_empty(), ErrorCode::InvalidChainId);

    require!(
        client_state.trust_level_numerator > 0
            && client_state.trust_level_numerator <= client_state.trust_level_denominator
            && client_state.trust_level_denominator > 0,
        ErrorCode::InvalidTrustLevel
    );

    require!(
        client_state.trusting_period > 0
            && client_state.unbonding_period > 0
            && client_state.trusting_period < client_state.unbonding_period,
        ErrorCode::InvalidPeriods
    );

    require!(
        client_state.max_clock_drift > 0,
        ErrorCode::InvalidMaxClockDrift
    );

    require!(
        client_state.latest_height.revision_height > 0,
        ErrorCode::InvalidHeight
    );

    let client_state_account = &mut ctx.accounts.client_state;
    let latest_height = client_state.latest_height;
    client_state_account.set_inner(client_state);

    let consensus_state_store = &mut ctx.accounts.consensus_state_store;
    consensus_state_store.height = latest_height.revision_height;
    consensus_state_store.consensus_state = consensus_state;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::fixtures::*;
    use anchor_lang::{AnchorDeserialize, InstructionData};
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    struct TestAccounts {
        payer: Pubkey,
        client_state_pda: Pubkey,
        consensus_state_store_pda: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts(latest_height: u64) -> TestAccounts {
        let payer = Pubkey::new_unique();
        let (client_state_pda, _) = Pubkey::find_program_address(&[b"client"], &crate::ID);
        let (consensus_state_store_pda, _) = Pubkey::find_program_address(
            &[b"consensus_state", &latest_height.to_le_bytes()],
            &crate::ID,
        );

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

        TestAccounts {
            payer,
            client_state_pda,
            consensus_state_store_pda,
            accounts,
        }
    }

    fn create_initialize_instruction(
        test_accounts: &TestAccounts,
        client_state: &ClientState,
        consensus_state: &ConsensusState,
    ) -> Instruction {
        let instruction_data = crate::instruction::Initialize {
            latest_height: client_state.latest_height.revision_height,
            client_state: client_state.clone(),
            consensus_state: consensus_state.clone(),
        };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(test_accounts.client_state_pda, false),
                AccountMeta::new(test_accounts.consensus_state_store_pda, false),
                AccountMeta::new(test_accounts.payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    fn assert_instruction_fails_with_error(
        instruction: &Instruction,
        accounts: &[(Pubkey, Account)],
        expected_error: ErrorCode,
    ) {
        let mollusk = Mollusk::new(&crate::ID, "../../target/deploy/ics07_tendermint");
        let result = mollusk.process_instruction(instruction, accounts);

        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                panic!("Expected instruction to fail with {expected_error:?}, but it succeeded");
            }
            mollusk_svm::result::ProgramResult::Failure(error) => {
                assert_eq!(
                    error,
                    anchor_lang::error::Error::from(expected_error).into()
                );
            }
            mollusk_svm::result::ProgramResult::UnknownError(error) => {
                panic!("Unknown error occurred: {error:?}");
            }
        }
    }

    fn test_initialize_validation_failure(
        mut client_state: ClientState,
        consensus_state: ConsensusState,
        setup_invalid_state: impl FnOnce(&mut ClientState),
        expected_error: ErrorCode,
    ) {
        // Store original height before modification
        let original_height = client_state.latest_height.revision_height;

        // Apply the invalid state modification
        setup_invalid_state(&mut client_state);

        // Use original height for PDA derivation to ensure consistency
        let test_accounts = setup_test_accounts(original_height);
        let instruction =
            create_initialize_instruction(&test_accounts, &client_state, &consensus_state);

        assert_instruction_fails_with_error(&instruction, &test_accounts.accounts, expected_error);
    }

    #[test]
    fn test_initialize_happy_path() {
        // Load all fixtures efficiently (single JSON parse)
        let (client_state, consensus_state, _) = load_primary_fixtures();

        let payer = Pubkey::new_unique();

        let (client_state_pda, _) = Pubkey::find_program_address(&[b"client"], &crate::ID);

        let latest_height = client_state.latest_height.revision_height;
        let (consensus_state_store_pda, _) = Pubkey::find_program_address(
            &[b"consensus_state", &latest_height.to_le_bytes()],
            &crate::ID,
        );

        let instruction_data = crate::instruction::Initialize {
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

        let payer_lamports = 10_000_000_000;
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
                    lamports: payer_lamports,
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

        let mollusk = Mollusk::new(&crate::ID, "../../target/deploy/ics07_tendermint");

        let checks = vec![
            Check::success(),
            Check::account(&client_state_pda).owner(&crate::ID).build(),
            Check::account(&consensus_state_store_pda)
                .owner(&crate::ID)
                .build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let payer_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &payer)
            .map(|(_, account)| account)
            .expect("Payer account not found");

        assert!(
            payer_account.lamports < payer_lamports,
            "Payer should have paid for account creation"
        );

        let client_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_state_pda)
            .map(|(_, account)| account)
            .expect("Client state account not found");

        assert!(
            client_state_account.lamports > 0,
            "Client state account should be rent-exempt"
        );
        assert!(
            client_state_account.data.len() > 8,
            "Client state account should have data"
        );

        let mut data_slice = &client_state_account.data[8..];
        let deserialized_client_state: ClientState =
            ClientState::deserialize(&mut data_slice).expect("Failed to deserialize client state");

        assert_eq!(deserialized_client_state.chain_id, client_state.chain_id);
        assert_eq!(
            deserialized_client_state.trust_level_numerator,
            client_state.trust_level_numerator
        );
        assert_eq!(
            deserialized_client_state.trust_level_denominator,
            client_state.trust_level_denominator
        );
        assert_eq!(
            deserialized_client_state.trusting_period,
            client_state.trusting_period
        );
        assert_eq!(
            deserialized_client_state.unbonding_period,
            client_state.unbonding_period
        );
        assert_eq!(
            deserialized_client_state.max_clock_drift,
            client_state.max_clock_drift
        );
        assert_eq!(
            deserialized_client_state.frozen_height.revision_number,
            client_state.frozen_height.revision_number
        );
        assert_eq!(
            deserialized_client_state.frozen_height.revision_height,
            client_state.frozen_height.revision_height
        );
        assert_eq!(
            deserialized_client_state.latest_height.revision_number,
            client_state.latest_height.revision_number
        );
        assert_eq!(
            deserialized_client_state.latest_height.revision_height,
            client_state.latest_height.revision_height
        );

        let consensus_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &consensus_state_store_pda)
            .map(|(_, account)| account)
            .expect("Consensus state store account not found");

        assert!(
            consensus_state_account.lamports > 0,
            "Consensus state store account should be rent-exempt"
        );
        assert!(
            consensus_state_account.data.len() > 8,
            "Consensus state store account should have data"
        );

        let mut data_slice = &consensus_state_account.data[8..];
        let deserialized_consensus_store: ConsensusStateStore =
            ConsensusStateStore::deserialize(&mut data_slice)
                .expect("Failed to deserialize consensus state store");

        assert_eq!(deserialized_consensus_store.height, latest_height);
        assert_eq!(
            deserialized_consensus_store.consensus_state.timestamp,
            consensus_state.timestamp
        );
        assert_eq!(
            deserialized_consensus_store.consensus_state.root,
            consensus_state.root
        );
        assert_eq!(
            deserialized_consensus_store
                .consensus_state
                .next_validators_hash,
            consensus_state.next_validators_hash
        );
    }

    // Chain ID validation test removed - single client per program, no chain_id parameter

    #[test]
    fn test_initialize_invalid_trust_level_zero_numerator() {
        let (client_state, consensus_state, _) = load_primary_fixtures();

        test_initialize_validation_failure(
            client_state,
            consensus_state,
            |cs| cs.trust_level_numerator = 0,
            ErrorCode::InvalidTrustLevel,
        );
    }

    #[test]
    fn test_initialize_invalid_trust_level_numerator_greater_than_denominator() {
        let (client_state, consensus_state, _) = load_primary_fixtures();

        test_initialize_validation_failure(
            client_state,
            consensus_state,
            |cs| {
                cs.trust_level_numerator = 5;
                cs.trust_level_denominator = 3;
            },
            ErrorCode::InvalidTrustLevel,
        );
    }

    #[test]
    fn test_initialize_invalid_trust_level_zero_denominator() {
        let (client_state, consensus_state, _) = load_primary_fixtures();

        test_initialize_validation_failure(
            client_state,
            consensus_state,
            |cs| cs.trust_level_denominator = 0,
            ErrorCode::InvalidTrustLevel,
        );
    }

    #[test]
    fn test_initialize_invalid_periods_zero_trusting_period() {
        let (client_state, consensus_state, _) = load_primary_fixtures();

        test_initialize_validation_failure(
            client_state,
            consensus_state,
            |cs| cs.trusting_period = 0,
            ErrorCode::InvalidPeriods,
        );
    }

    #[test]
    fn test_initialize_invalid_periods_zero_unbonding_period() {
        let (client_state, consensus_state, _) = load_primary_fixtures();

        test_initialize_validation_failure(
            client_state,
            consensus_state,
            |cs| cs.unbonding_period = 0,
            ErrorCode::InvalidPeriods,
        );
    }

    #[test]
    fn test_initialize_invalid_periods_trusting_greater_than_unbonding() {
        let (client_state, consensus_state, _) = load_primary_fixtures();

        test_initialize_validation_failure(
            client_state,
            consensus_state,
            |cs| {
                cs.trusting_period = 1000;
                cs.unbonding_period = 500;
            },
            ErrorCode::InvalidPeriods,
        );
    }

    #[test]
    fn test_initialize_invalid_max_clock_drift() {
        let (client_state, consensus_state, _) = load_primary_fixtures();

        test_initialize_validation_failure(
            client_state,
            consensus_state,
            |cs| cs.max_clock_drift = 0,
            ErrorCode::InvalidMaxClockDrift,
        );
    }

    #[test]
    fn test_initialize_invalid_height() {
        let (mut client_state, consensus_state, _) = load_primary_fixtures();

        // For height test, we need special handling since modifying height affects PDA derivation
        client_state.latest_height.revision_height = 0;

        let test_accounts = setup_test_accounts(0); // Use the modified height for PDA
        let instruction =
            create_initialize_instruction(&test_accounts, &client_state, &consensus_state);

        assert_instruction_fails_with_error(
            &instruction,
            &test_accounts.accounts,
            ErrorCode::InvalidHeight,
        );
    }
}
