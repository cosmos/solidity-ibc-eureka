use crate::error::ErrorCode;
use crate::helpers::deserialize_header;
use crate::state::ConsensusStateStore;
use crate::types::{ClientState, ConsensusState, UpdateClientMsg, UpdateResult};
use crate::UpdateClient;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::pubkey::Pubkey;
use anchor_lang::system_program;
use ibc_client_tendermint::types::ConsensusState as IbcConsensusState;
use ibc_core_client_types::Height;
use tendermint_light_client_update_client::ClientState as UpdateClientState;

/// Size of Anchor's account discriminator in bytes
const ANCHOR_DISCRIMINATOR_SIZE: usize = 8;

struct ConsensusStateStorageContext<'info, 'a> {
    new_consensus_state_store: &'a UncheckedAccount<'info>,
    payer: &'a Signer<'info>,
    system_program: &'a Program<'info, System>,
    program_id: &'a Pubkey,
    client_key: Pubkey,
    revision_height: u64,
}

pub fn update_client(ctx: Context<UpdateClient>, msg: UpdateClientMsg) -> Result<UpdateResult> {
    let client_state = &mut ctx.accounts.client_state;

    require!(!client_state.is_frozen(), ErrorCode::ClientFrozen);

    let header = deserialize_header(&msg.client_message)?;
    let trusted_height = header.trusted_height;

    let trusted_consensus_state = validate_and_load_trusted_state(
        &ctx.accounts.trusted_consensus_state,
        client_state.key(),
        trusted_height.revision_height(),
        ctx.program_id,
    )?;

    let (new_height, new_consensus_state) = verify_header_and_get_state(
        client_state.as_ref(),
        &trusted_consensus_state.consensus_state,
        &msg.client_message,
    )?;

    if new_consensus_state.timestamp <= trusted_consensus_state.consensus_state.timestamp {
        client_state.freeze();
        return err!(ErrorCode::MisbehaviourNonIncreasingTime);
    }

    verify_consensus_state_pda(
        &ctx.accounts.new_consensus_state_store,
        client_state.key(),
        new_height.revision_height(),
        ctx.program_id,
    )?;

    let storage_context = ConsensusStateStorageContext {
        new_consensus_state_store: &ctx.accounts.new_consensus_state_store,
        payer: &ctx.accounts.payer,
        system_program: &ctx.accounts.system_program,
        program_id: ctx.program_id,
        client_key: client_state.key(),
        revision_height: new_height.revision_height(),
    };

    let update_result =
        handle_consensus_state_storage(storage_context, &new_consensus_state, client_state)?;

    if update_result == UpdateResult::Update {
        client_state.latest_height = new_height.into();
    }

    Ok(update_result)
}

fn validate_and_load_trusted_state(
    trusted_consensus_state_account: &UncheckedAccount<'_>,
    client_key: Pubkey,
    trusted_height: u64,
    program_id: &Pubkey,
) -> Result<ConsensusStateStore> {
    // Validate the PDA
    let (expected_pda, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_key.as_ref(),
            &trusted_height.to_le_bytes(),
        ],
        program_id,
    );

    require!(
        expected_pda == trusted_consensus_state_account.key(),
        ErrorCode::AccountValidationFailed
    );

    // Load and verify the account exists
    let account_data = trusted_consensus_state_account.try_borrow_data()?;
    require!(!account_data.is_empty(), ErrorCode::ConsensusStateNotFound);

    // Deserialize the consensus state (include discriminator for proper validation)
    ConsensusStateStore::try_deserialize(&mut &account_data[..])
        .map_err(|_e| error!(ErrorCode::SerializationError))
}

fn verify_header_and_get_state(
    client_state: &ClientState,
    consensus_state: &ConsensusState,
    client_message: &[u8],
) -> Result<(Height, ConsensusState)> {
    let header = deserialize_header(client_message)?;

    let update_client_state: UpdateClientState = client_state.clone().into();
    let trusted_consensus_state: IbcConsensusState = consensus_state.clone().into();
    let current_time = Clock::get()?.unix_timestamp as u128 * 1_000_000_000;

    let output = tendermint_light_client_update_client::update_client(
        &update_client_state,
        &trusted_consensus_state,
        header,
        current_time,
    )
    .map_err(|e| match e {
        tendermint_light_client_update_client::UpdateClientError::HeaderVerificationFailed => {
            error!(ErrorCode::HeaderVerificationFailed)
        }
        _ => {
            error!(ErrorCode::UpdateClientFailed)
        }
    })?;

    Ok((
        output.latest_height,
        output
            .new_consensus_state
            .try_into()
            .map_err(|_| error!(ErrorCode::InvalidRootLength))?,
    ))
}

fn verify_consensus_state_pda(
    new_consensus_state_store: &UncheckedAccount,
    client_key: Pubkey,
    revision_height: u64,
    program_id: &Pubkey,
) -> Result<()> {
    let (expected_pda, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_key.as_ref(),
            &revision_height.to_le_bytes(),
        ],
        program_id,
    );

    require!(
        expected_pda == new_consensus_state_store.key(),
        ErrorCode::AccountValidationFailed
    );

    Ok(())
}

fn handle_consensus_state_storage(
    ctx: ConsensusStateStorageContext,
    new_consensus_state: &ConsensusState,
    client_state: &mut ClientState,
) -> Result<UpdateResult> {
    if ctx.new_consensus_state_store.data_is_empty() {
        // Create new consensus state account
        create_consensus_state_account(
            ctx.new_consensus_state_store,
            ctx.payer,
            ctx.system_program,
            CreateConsensusStateParams {
                program_id: ctx.program_id,
                client_key: ctx.client_key,
                revision_height: ctx.revision_height,
                new_consensus_state,
            },
            client_state,
        )?;
        Ok(UpdateResult::Update)
    } else {
        // Consensus state already exists at this height - check for misbehaviour
        check_existing_consensus_state(
            ctx.new_consensus_state_store,
            new_consensus_state,
            client_state,
        )
    }
}

fn check_existing_consensus_state(
    new_consensus_state_store: &UncheckedAccount,
    new_consensus_state: &ConsensusState,
    client_state: &mut ClientState,
) -> Result<UpdateResult> {
    let data = new_consensus_state_store.try_borrow_data()?;
    let existing_store: ConsensusStateStore = ConsensusStateStore::try_deserialize(&mut &data[..])
        .map_err(|_| error!(ErrorCode::SerializationError))?;

    if &existing_store.consensus_state != new_consensus_state {
        client_state.freeze();
        return err!(ErrorCode::MisbehaviourConflictingState);
    }

    Ok(UpdateResult::NoOp)
}

/// Creates seeds for deriving consensus state store PDAs
fn consensus_state_seeds(client_key: &Pubkey, revision_height: u64) -> [Vec<u8>; 3] {
    [
        b"consensus_state".to_vec(),
        client_key.as_ref().to_vec(),
        revision_height.to_le_bytes().to_vec(),
    ]
}

/// Generic function to convert seed vectors to slices
#[inline]
fn vecs_as_slices<const N: usize>(seeds: &[Vec<u8>; N]) -> [&[u8]; N] {
    std::array::from_fn(|i| seeds[i].as_slice())
}

/// Validates that the provided account matches the expected PDA for consensus state storage
fn validate_consensus_state_pda(
    consensus_state_account: &UncheckedAccount,
    client_key: &Pubkey,
    revision_height: u64,
    program_id: &Pubkey,
) -> Result<u8> {
    let seeds = consensus_state_seeds(client_key, revision_height);
    let seeds_slices = vecs_as_slices(&seeds);
    let (expected_pda, bump) = Pubkey::find_program_address(&seeds_slices, program_id);

    require!(
        expected_pda == consensus_state_account.key(),
        ErrorCode::AccountValidationFailed
    );

    Ok(bump)
}

/// Calculates the required space and rent for a consensus state store account
fn calculate_consensus_state_rent() -> Result<(usize, u64)> {
    let space = ANCHOR_DISCRIMINATOR_SIZE + ConsensusStateStore::INIT_SPACE;
    let rent = Rent::get()?.minimum_balance(space);
    Ok((space, rent))
}

/// Creates signer seeds for consensus state PDA with the bump
fn create_consensus_state_signer_seeds(
    client_key: &Pubkey,
    revision_height: u64,
    bump: u8,
) -> [Vec<u8>; 4] {
    [
        b"consensus_state".to_vec(),
        client_key.as_ref().to_vec(),
        revision_height.to_le_bytes().to_vec(),
        vec![bump],
    ]
}

/// Helper function to create account using system program with PDA signing
fn create_account_with_system_program<'info>(
    new_account: &UncheckedAccount<'info>,
    payer: &Signer<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
    space: usize,
    rent: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    system_program::create_account(
        CpiContext::new_with_signer(
            system_program.to_account_info(),
            system_program::CreateAccount {
                from: payer.to_account_info(),
                to: new_account.to_account_info(),
            },
            signer_seeds,
        ),
        rent,
        space as u64,
        program_id,
    )
}

/// Initializes the consensus state store with the provided data
fn initialize_consensus_state_store(
    consensus_state_account: &UncheckedAccount<'_>,
    revision_height: u64,
    consensus_state: &ConsensusState,
) -> Result<()> {
    let mut data = consensus_state_account.try_borrow_mut_data()?;
    let mut cursor = std::io::Cursor::new(&mut data[..]);

    let consensus_state_store = ConsensusStateStore {
        height: revision_height,
        consensus_state: consensus_state.clone(),
    };

    // Use try_serialize which handles both discriminator and data serialization
    consensus_state_store.try_serialize(&mut cursor)?;
    Ok(())
}

/// Creates and initializes a new consensus state store account
/// Parameters for creating a new consensus state account
struct CreateConsensusStateParams<'a> {
    program_id: &'a Pubkey,
    client_key: Pubkey,
    revision_height: u64,
    new_consensus_state: &'a ConsensusState,
}

fn create_consensus_state_account<'info>(
    consensus_state_account: &UncheckedAccount<'info>,
    payer: &Signer<'info>,
    system_program: &Program<'info, System>,
    params: CreateConsensusStateParams<'_>,
    client_state: &mut ClientState,
) -> Result<()> {
    // Validate the PDA and get the bump seed
    let bump = validate_consensus_state_pda(
        consensus_state_account,
        &params.client_key,
        params.revision_height,
        params.program_id,
    )?;

    // Calculate required space and rent
    let (space, rent) = calculate_consensus_state_rent()?;

    // Prepare signing seeds for account creation
    let signer_seeds = create_consensus_state_signer_seeds(&params.client_key, params.revision_height, bump);
    let signer_seeds_slices = vecs_as_slices(&signer_seeds);
    let signer_seeds_slice = [&signer_seeds_slices[..]];

    // Create the account with system program
    create_account_with_system_program(
        consensus_state_account,
        payer,
        system_program,
        params.program_id,
        space,
        rent,
        &signer_seeds_slice,
    )?;

    // Initialize the consensus state store data
    initialize_consensus_state_store(
        consensus_state_account,
        params.revision_height,
        params.new_consensus_state,
    )?;

    // Update consensus state tracking for pruning
    client_state.consensus_state_count = client_state.consensus_state_count.saturating_add(1);

    // Check if we need to signal for pruning
    if client_state.consensus_state_count > client_state.max_consensus_states {
        // NOTE: Actual pruning will be handled by a separate instruction
        // This is because we need an additional account (the oldest consensus state)
        // which isn't available in the current UpdateClient context.
        // The pruning can be triggered by anyone to reclaim rent as an incentive.

        // Update earliest_height to the next one that should be kept
        // This signals that consensus states below this height can be pruned
        let states_to_keep = u64::from(client_state.max_consensus_states);
        let approx_new_earliest = params.revision_height.saturating_sub(states_to_keep);

        // Only update if moving forward (never go backwards)
        if approx_new_earliest > client_state.earliest_height {
            client_state.earliest_height = approx_new_earliest;
        }

        msg!(
            "Consensus state window exceeded: count={}, max={}, earliest_height can be pruned up to {}",
            client_state.consensus_state_count,
            client_state.max_consensus_states,
            client_state.earliest_height
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::{fixtures::*, PROGRAM_BINARY_PATH};
    use crate::types::UpdateClientMsg;
    use anchor_lang::{AnchorDeserialize, InstructionData};
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::sysvar::clock::{Clock, ID as CLOCK_ID};
    use solana_sdk::{native_loader, system_program};

    pub struct InitializedClientResult {
        pub client_state_pda: Pubkey,
        pub consensus_state_store_pda: Pubkey,
        pub payer: Pubkey,
        pub client_state: ClientState,
        pub consensus_state: ConsensusState,
        pub resulting_accounts: Vec<(Pubkey, Account)>,
    }

    pub struct UpdateClientTestScenario {
        pub client_state_pda: Pubkey,
        pub trusted_consensus_state_pda: Pubkey,
        pub new_consensus_state_pda: Pubkey,
        pub payer: Pubkey,
        pub instruction: Instruction,
        pub accounts: Vec<(Pubkey, Account)>,
    }

    pub struct HappyPathTestScenario {
        pub client_state_pda: Pubkey,
        pub trusted_consensus_state_pda: Pubkey,
        pub new_consensus_state_pda: Pubkey,
        pub payer: Pubkey,
        pub instruction: Instruction,
        pub accounts: Vec<(Pubkey, Account)>,
        pub update_message: UpdateClientMessage,
    }

    fn create_clock_account(unix_timestamp: i64) -> (Pubkey, Account) {
        let clock = Clock {
            slot: 1000,
            epoch_start_timestamp: 0,
            epoch: 1,
            leader_schedule_epoch: 1,
            unix_timestamp,
        };

        // Serialize the Clock struct using bincode
        let data = bincode::serialize(&clock).expect("Failed to serialize Clock");

        (
            CLOCK_ID,
            Account {
                lamports: 1,
                data,
                owner: solana_sdk::sysvar::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
    }

    fn create_update_client_instruction(
        client_state_pda: Pubkey,
        trusted_consensus_state_pda: Pubkey,
        new_consensus_state_pda: Pubkey,
        payer: Pubkey,
        client_message: Vec<u8>,
    ) -> Instruction {
        let update_msg = UpdateClientMsg { client_message };
        let instruction_data = crate::instruction::UpdateClient { msg: update_msg };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new_readonly(trusted_consensus_state_pda, false),
                AccountMeta::new(new_consensus_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        }
    }

    fn create_empty_consensus_state_account() -> Account {
        Account {
            lamports: 0,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    fn setup_test_accounts_with_new_consensus_state(
        initialized_accounts: Vec<(Pubkey, Account)>,
        new_consensus_state_pda: Pubkey,
        payer: Pubkey,
        payer_lamports: u64,
    ) -> Vec<(Pubkey, Account)> {
        let mut accounts = initialized_accounts;
        accounts.push((
            new_consensus_state_pda,
            create_empty_consensus_state_account(),
        ));

        // Update payer lamports
        if let Some((_, account)) = accounts.iter_mut().find(|(key, _)| *key == payer) {
            account.lamports = payer_lamports;
        }

        accounts
    }

    fn execute_update_client_instruction(
        instruction: &Instruction,
        accounts: &[(Pubkey, Account)],
    ) -> mollusk_svm::result::InstructionResult {
        let mut mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        mollusk.compute_budget.compute_unit_limit = 20_000_000;
        mollusk.process_instruction(instruction, accounts)
    }

    fn find_account_in_result<'a>(
        result: &'a mollusk_svm::result::InstructionResult,
        target_pubkey: &Pubkey,
    ) -> &'a Account {
        result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == target_pubkey)
            .map_or_else(
                || panic!("Account {target_pubkey} not found"),
                |(_, account)| account,
            )
    }

    fn setup_update_client_test_scenario(
        client_message: Vec<u8>,
        new_height: u64,
        custom_accounts: Option<Vec<(Pubkey, Account)>>,
    ) -> UpdateClientTestScenario {
        let initialized_client = setup_initialized_client();
        let client_state_pda = initialized_client.client_state_pda;
        let trusted_consensus_state_pda = initialized_client.consensus_state_store_pda;
        let payer = initialized_client.payer;
        let initialized_accounts = initialized_client.resulting_accounts;

        let (new_consensus_state_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state_pda.as_ref(),
                &new_height.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction = create_update_client_instruction(
            client_state_pda,
            trusted_consensus_state_pda,
            new_consensus_state_pda,
            payer,
            client_message,
        );

        let accounts = custom_accounts.unwrap_or_else(|| {
            setup_test_accounts_with_new_consensus_state(
                initialized_accounts,
                new_consensus_state_pda,
                payer,
                100_000_000_000,
            )
        });

        UpdateClientTestScenario {
            client_state_pda,
            trusted_consensus_state_pda,
            new_consensus_state_pda,
            payer,
            instruction,
            accounts,
        }
    }

    // Helper function to setup a standard happy path test scenario
    fn setup_happy_path_test_scenario() -> HappyPathTestScenario {
        let update_message = load_update_client_message("update_client_happy_path");
        let client_message = hex_to_bytes(&update_message.client_message_hex);
        let new_height = update_message.new_height;

        let test_scenario = setup_update_client_test_scenario(client_message, new_height, None);

        HappyPathTestScenario {
            client_state_pda: test_scenario.client_state_pda,
            trusted_consensus_state_pda: test_scenario.trusted_consensus_state_pda,
            new_consensus_state_pda: test_scenario.new_consensus_state_pda,
            payer: test_scenario.payer,
            instruction: test_scenario.instruction,
            accounts: test_scenario.accounts,
            update_message,
        }
    }

    fn setup_initialized_client() -> InitializedClientResult {
        // Load from primary fixtures efficiently (single JSON parse)
        let (client_state, consensus_state, update_message) = load_primary_fixtures();

        let chain_id = &client_state.chain_id;
        let payer = Pubkey::new_unique();

        let (client_state_pda, _) =
            Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], &crate::ID);

        let latest_height = client_state.latest_height.revision_height;
        let (consensus_state_store_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state_pda.as_ref(),
                &latest_height.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::Initialize {
            chain_id: chain_id.clone(),
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

        // Create clock account with timestamp based on fixture data
        let clock_timestamp = get_valid_clock_timestamp_for_header(&update_message);
        let (clock_pubkey, clock_account) = create_clock_account(clock_timestamp);

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
            (clock_pubkey, clock_account),
        ];

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);

        let checks = vec![
            Check::success(),
            Check::account(&client_state_pda).owner(&crate::ID).build(),
            Check::account(&consensus_state_store_pda)
                .owner(&crate::ID)
                .build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        // Return the resulting accounts from the initialize instruction
        InitializedClientResult {
            client_state_pda,
            consensus_state_store_pda,
            payer,
            client_state,
            consensus_state,
            resulting_accounts: result.resulting_accounts,
        }
    }

    #[test]
    fn test_update_client_happy_path() {
        let scenario = setup_happy_path_test_scenario();

        let new_height = scenario.update_message.new_height;
        let result = execute_update_client_instruction(&scenario.instruction, &scenario.accounts);

        // Check if the instruction succeeded
        match result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                // Continue with test validation
            }
            _ => {
                panic!(
                    "Update client instruction failed: {:?}",
                    result.program_result
                );
            }
        }

        // Verify the client state was updated
        let client_state_account = find_account_in_result(&result, &scenario.client_state_pda);
        let mut data_slice = &client_state_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let updated_client_state: ClientState =
            ClientState::deserialize(&mut data_slice).expect("Failed to deserialize client state");

        // Verify the client state updates
        assert_eq!(
            updated_client_state.latest_height.revision_height, new_height,
            "Client state latest height should be updated to new height"
        );
        assert_eq!(
            updated_client_state.latest_height.revision_number, 2,
            "Revision number should remain the same"
        );
        assert!(
            !updated_client_state.is_frozen(),
            "Client should not be frozen after successful update"
        );

        // Verify the new consensus state was created and is properly configured
        let new_consensus_state_account =
            find_account_in_result(&result, &scenario.new_consensus_state_pda);
        assert!(
            new_consensus_state_account.lamports > 0,
            "New consensus state account should be rent-exempt"
        );
        assert!(
            new_consensus_state_account.data.len() > 8,
            "New consensus state account should have data"
        );
        assert_eq!(
            new_consensus_state_account.owner,
            crate::ID,
            "New consensus state should be owned by our program"
        );

        // Verify the consensus state store structure
        let mut data_slice = &new_consensus_state_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
        let new_consensus_store: ConsensusStateStore =
            ConsensusStateStore::deserialize(&mut data_slice)
                .expect("Failed to deserialize new consensus state store");

        assert_eq!(
            new_consensus_store.height, new_height,
            "Consensus state store height should match new height"
        );

        // Verify the consensus state contains valid data
        let consensus_state = &new_consensus_store.consensus_state;
        assert!(
            consensus_state.timestamp > 0,
            "Consensus state should have a valid timestamp"
        );
        assert_eq!(
            consensus_state.root.len(),
            32,
            "Root hash should be 32 bytes"
        );
        assert_eq!(
            consensus_state.next_validators_hash.len(),
            32,
            "Next validators hash should be 32 bytes"
        );

        // Verify the trusted consensus state account is unchanged
        let trusted_consensus_state_account =
            find_account_in_result(&result, &scenario.trusted_consensus_state_pda);
        assert_eq!(
            trusted_consensus_state_account.data.len(),
            88,
            "Trusted consensus state should remain unchanged"
        );

        // Verify payer account was charged for creating the new consensus state
        let payer_account = find_account_in_result(&result, &scenario.payer);
        assert!(
            payer_account.lamports < 100_000_000_000,
            "Payer should have been charged for account creation"
        );

        // Verify account discriminators are correct
        assert_eq!(
            &client_state_account.data[0..ANCHOR_DISCRIMINATOR_SIZE],
            crate::types::ClientState::DISCRIMINATOR,
            "Client state should have correct discriminator"
        );
        assert_eq!(
            &new_consensus_state_account.data[0..ANCHOR_DISCRIMINATOR_SIZE],
            ConsensusStateStore::DISCRIMINATOR,
            "New consensus state should have correct discriminator"
        );

        println!("✅ All state updates verified successfully:");
        println!("  - Client state latest height: {} -> {}", 19, new_height);
        println!("  - New consensus state created at height: {new_height}");
        println!(
            "  - Consensus state timestamp: {}",
            consensus_state.timestamp
        );
        println!(
            "  - Payer charged: {} lamports",
            100_000_000_000 - payer_account.lamports
        );
    }

    #[test]
    fn test_update_client_with_malformed_header() {
        // Load happy path and corrupt the signature
        let update_message = load_update_client_message("update_client_happy_path");
        let malformed_message = corrupt_header_signature(&update_message.client_message_hex);
        let new_height = update_message.new_height;

        // CRITICAL TEST: Verify that the malformed message can be deserialized
        // This proves we're testing cryptographic validation, not protobuf parsing
        let _parsed_header = crate::helpers::deserialize_header(&malformed_message)
            .expect("CRITICAL: Malformed header MUST deserialize successfully. If this fails, the test is testing parsing instead of validation!");

        println!("✅ Malformed header deserialized successfully - testing cryptographic validation, not parsing");

        let scenario = setup_update_client_test_scenario(malformed_message, new_height, None);

        let result = execute_update_client_instruction(&scenario.instruction, &scenario.accounts);

        // Should fail with HeaderVerificationFailed (cryptographic validation failure)
        assert_error_code(
            result,
            ErrorCode::HeaderVerificationFailed,
            "Malformed header",
        );

        println!("✅ Test passed: Malformed header failed cryptographic validation (not deserialization)");
    }

    #[test]
    fn test_update_client_with_invalid_protobuf_bytes() {
        // Test with completely invalid protobuf bytes to trigger InvalidHeader (6008)
        let invalid_protobuf_bytes = vec![0xFF, 0xFF, 0xFF, 0xFF]; // Invalid protobuf data
        let dummy_height = 123;

        let scenario =
            setup_update_client_test_scenario(invalid_protobuf_bytes, dummy_height, None);

        let result = execute_update_client_instruction(&scenario.instruction, &scenario.accounts);

        // Should fail with InvalidHeader (protobuf parsing failure)
        assert_error_code(result, ErrorCode::InvalidHeader, "Invalid protobuf");

        println!("✅ Test passed: Invalid protobuf bytes correctly returned InvalidHeader (6008)");
    }

    #[test]
    fn test_update_client_with_wrong_trusted_height() {
        let update_message = load_update_client_message("update_client_happy_path");

        // Manipulate the header to have wrong trusted height
        let wrong_height = update_message.trusted_height.saturating_add(100);
        let client_message = create_message_with_wrong_trusted_height(
            &update_message.client_message_hex,
            wrong_height,
        );
        let new_height = update_message.new_height;

        let scenario = setup_update_client_test_scenario(client_message.clone(), new_height, None);
        let mut accounts = scenario.accounts;

        // Use wrong trusted consensus state PDA
        let (wrong_trusted_consensus_state_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                scenario.client_state_pda.as_ref(),
                &wrong_height.to_le_bytes(),
            ],
            &crate::ID,
        );

        let (new_consensus_state_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                scenario.client_state_pda.as_ref(),
                &new_height.to_le_bytes(),
            ],
            &crate::ID,
        );

        // Create instruction with wrong trusted consensus state
        let instruction = create_update_client_instruction(
            scenario.client_state_pda,
            wrong_trusted_consensus_state_pda, // Wrong trusted state
            new_consensus_state_pda,
            scenario.payer,
            client_message,
        );

        // Add empty account for wrong trusted state
        accounts.push((
            wrong_trusted_consensus_state_pda,
            create_empty_consensus_state_account(),
        ));

        let result = execute_update_client_instruction(&instruction, &accounts);
        // Should fail because consensus state doesn't exist at wrong height
        assert_error_code(
            result,
            ErrorCode::ConsensusStateNotFound,
            "Wrong trusted height",
        );
    }

    #[test]
    fn test_update_client_with_expired_header() {
        let scenario = setup_happy_path_test_scenario();

        // Use a clock time that's way in the future to make header appear expired
        let future_timestamp = get_expired_clock_timestamp_for_header(&scenario.update_message);
        let (clock_pubkey, clock_account) = create_clock_account(future_timestamp);

        // Replace the clock account
        let mut accounts = scenario.accounts;
        if let Some((_, acc)) = accounts.iter_mut().find(|(key, _)| *key == clock_pubkey) {
            *acc = clock_account;
        }

        let result = execute_update_client_instruction(&scenario.instruction, &accounts);
        // Should fail with header verification failure due to expiry
        assert_error_code(
            result,
            ErrorCode::HeaderVerificationFailed,
            "Expired header",
        );
    }

    #[test]
    fn test_update_client_with_duplicate_consensus_state() {
        // First, create a consensus state at the target height
        let scenario = setup_happy_path_test_scenario();

        // Execute the first update (should succeed)
        let first_result =
            execute_update_client_instruction(&scenario.instruction, &scenario.accounts);
        match first_result.program_result {
            mollusk_svm::result::ProgramResult::Success => {}
            _ => panic!(
                "First update should succeed: {:?}",
                first_result.program_result
            ),
        }

        // Now try to update with the same height again (should return NoOp)
        let second_result = execute_update_client_instruction(
            &scenario.instruction,
            &first_result.resulting_accounts,
        );
        match second_result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!("✅ Duplicate consensus state correctly handled as NoOp");
            }
            _ => panic!(
                "Second update with same height should succeed as NoOp: {:?}",
                second_result.program_result
            ),
        }
    }

    #[test]
    fn test_update_client_with_conflicting_consensus_state() {
        // Use happy path to create first consensus state
        let scenario = setup_happy_path_test_scenario();
        let new_height = scenario.update_message.new_height;

        // Execute first update to create initial consensus state
        let first_result =
            execute_update_client_instruction(&scenario.instruction, &scenario.accounts);
        match first_result.program_result {
            mollusk_svm::result::ProgramResult::Success => {
                println!(
                    "✅ First update succeeded, consensus state created at height {new_height}"
                );
            }
            _ => panic!(
                "First update should succeed: {:?}",
                first_result.program_result
            ),
        }

        // Now create a different consensus state by manually creating one with different data
        // We'll manually populate the consensus state account with different data at the same height
        let mut modified_accounts = first_result.resulting_accounts;

        // Find the consensus state account and modify its data to create a conflict
        if let Some((_, account)) = modified_accounts
            .iter_mut()
            .find(|(key, _)| *key == scenario.new_consensus_state_pda)
        {
            let data = &mut account.data;

            if data.len() > 40 {
                // Discriminator (8) + height (8) + timestamp (8) = 24 bytes, root starts at byte 24
                let end_idx = data.len().min(32);
                for byte in &mut data[24..end_idx] {
                    *byte ^= 0xFF; // Flip bits to create different root
                }
                println!("✅ Modified consensus state data to create conflict");
            }
        }

        // Try to update again with the same valid message - should detect misbehaviour
        let second_result =
            execute_update_client_instruction(&scenario.instruction, &modified_accounts);

        // Should detect misbehaviour and fail
        assert_error_code(
            second_result,
            ErrorCode::MisbehaviourConflictingState,
            "Conflicting consensus state",
        );
    }

    #[test]
    fn test_update_client_with_frozen_client() {
        use crate::test_helpers::fixtures::load_primary_fixtures;

        let mut scenario = setup_happy_path_test_scenario();
        let (client_state, _, _) = load_primary_fixtures();

        // Create frozen client state
        let mut frozen_client_state = client_state;
        frozen_client_state.frozen_height = crate::types::IbcHeight {
            revision_number: 0,
            revision_height: 50, // Frozen at height 50
        };

        // Serialize with Anchor discriminator
        let mut frozen_client_data = vec![];
        frozen_client_state
            .try_serialize(&mut frozen_client_data)
            .expect("Failed to serialize frozen client state");

        // Replace the client state account
        if let Some((_, account)) = scenario
            .accounts
            .iter_mut()
            .find(|(key, _)| *key == scenario.client_state_pda)
        {
            account.data = frozen_client_data;
        }

        let result = execute_update_client_instruction(&scenario.instruction, &scenario.accounts);

        assert_error_code(result, ErrorCode::ClientFrozen, "frozen client");
    }
}
