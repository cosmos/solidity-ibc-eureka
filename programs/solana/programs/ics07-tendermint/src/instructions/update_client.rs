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
use std::io::Write;
use tendermint_light_client_update_client::ClientState as UpdateClientState;

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

    check_timestamp_misbehaviour(
        &new_consensus_state,
        &trusted_consensus_state.consensus_state,
        client_state,
    )?;

    verify_consensus_state_pda(
        &ctx.accounts.new_consensus_state_store,
        client_state.key(),
        new_height.revision_height(),
        ctx.program_id,
    )?;

    let update_result = handle_consensus_state_storage(
        &ctx.accounts.new_consensus_state_store,
        &ctx.accounts.payer,
        &ctx.accounts.system_program,
        ctx.program_id,
        new_height.revision_height(),
        &new_consensus_state,
        client_state,
    )?;

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

    // Deserialize the consensus state (skip 8-byte discriminator)
    ConsensusStateStore::try_deserialize(&mut &account_data[8..])
        .map_err(|_| error!(ErrorCode::SerializationError))
}

fn check_timestamp_misbehaviour(
    new_consensus_state: &ConsensusState,
    trusted_consensus_state: &ConsensusState,
    client_state: &mut ClientState,
) -> Result<()> {
    let trusted_timestamp = Into::<IbcConsensusState>::into(trusted_consensus_state.clone())
        .timestamp
        .unix_timestamp_nanos() as u64;

    if new_consensus_state.timestamp <= trusted_timestamp {
        client_state.freeze();
        msg!("Misbehaviour detected: non-increasing timestamp");
        return err!(ErrorCode::MisbehaviourNonIncreasingTime);
    }

    Ok(())
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
    .map_err(|e| {
        msg!("Update client failed: {:?}", e);
        error!(ErrorCode::UpdateClientFailed)
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

fn handle_consensus_state_storage<'info>(
    new_consensus_state_store: &UncheckedAccount<'info>,
    payer: &Signer<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
    revision_height: u64,
    new_consensus_state: &ConsensusState,
    client_state: &mut ClientState,
) -> Result<UpdateResult> {
    if new_consensus_state_store.data_is_empty() {
        // Create new consensus state account
        create_consensus_state_account(
            new_consensus_state_store,
            payer,
            system_program,
            program_id,
            revision_height,
            new_consensus_state,
        )?;
        Ok(UpdateResult::Update)
    } else {
        // Consensus state already exists at this height - check for misbehaviour
        check_existing_consensus_state(
            new_consensus_state_store,
            new_consensus_state,
            revision_height,
            client_state,
        )
    }
}

fn check_existing_consensus_state(
    new_consensus_state_store: &UncheckedAccount,
    new_consensus_state: &ConsensusState,
    revision_height: u64,
    client_state: &mut ClientState,
) -> Result<UpdateResult> {
    let data = new_consensus_state_store.try_borrow_data()?;
    let existing_store: ConsensusStateStore = ConsensusStateStore::try_deserialize(&mut &data[8..])
        .map_err(|_| error!(ErrorCode::SerializationError))?;

    if &existing_store.consensus_state != new_consensus_state {
        client_state.freeze();
        msg!(
            "Misbehaviour detected: conflicting consensus state at height {}",
            revision_height
        );

        return err!(ErrorCode::MisbehaviourConflictingState);
    }

    msg!(
        "Consensus state already exists at height {}",
        revision_height
    );
    Ok(UpdateResult::NoOp)
}

fn create_consensus_state_account<'info>(
    new_consensus_state_store: &UncheckedAccount<'info>,
    payer: &Signer<'info>,
    system_program: &Program<'info, System>,
    program_id: &Pubkey,
    revision_height: u64,
    new_consensus_state: &ConsensusState,
) -> Result<()> {
    let space = 8 + ConsensusStateStore::INIT_SPACE;
    let rent = Rent::get()?.minimum_balance(space);

    system_program::create_account(
        CpiContext::new(
            system_program.to_account_info(),
            system_program::CreateAccount {
                from: payer.to_account_info(),
                to: new_consensus_state_store.to_account_info(),
            },
        ),
        rent,
        space as u64,
        program_id,
    )?;

    let mut data = new_consensus_state_store.try_borrow_mut_data()?;
    let mut cursor = std::io::Cursor::new(&mut data[..]);

    // TODO: use build.rs to compute
    // NOTE: Anchor requires all accounts to start with an 8-byte discriminator that identifies
    // the account type. This is the SHA256 hash of "account:ConsensusStateStore" (first 8 bytes).
    // We write it manually here because we're creating the account using system_program::create_account
    // instead of Anchor's init constraint, which would normally handle this automatically.
    // We do manual creation to check for existing accounts first (for misbehaviour detection).
    let discriminator = [217, 208, 130, 233, 170, 148, 153, 101];
    cursor.write_all(&discriminator)?;

    let store = ConsensusStateStore {
        height: revision_height,
        consensus_state: new_consensus_state.clone(),
    };
    store.try_serialize(&mut cursor)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::fixtures::*;
    use crate::types::UpdateClientMsg;
    use anchor_lang::{AnchorDeserialize, InstructionData};
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    fn setup_initialized_client() -> (
        Pubkey,
        Pubkey,
        Pubkey,
        ClientState,
        ConsensusState,
        Vec<(Pubkey, Account)>,
    ) {
        let client_state_fixture = load_client_state_fixture();
        let consensus_state_fixture = load_consensus_state_fixture();

        let chain_id = &client_state_fixture.chain_id;
        let client_state = client_state_from_fixture(&client_state_fixture);
        let consensus_state = consensus_state_from_fixture(&consensus_state_fixture);

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

        // Return the resulting accounts from the initialize instruction
        (
            client_state_pda,
            consensus_state_store_pda,
            payer,
            client_state,
            consensus_state,
            result.resulting_accounts,
        )
    }

    #[test]
    fn test_update_client_happy_path() {
        let (client_state_pda, trusted_consensus_state_pda, payer, _, _, initialized_accounts) =
            setup_initialized_client();

        let update_message_fixture = load_update_client_message_fixture();
        let client_message = hex_to_bytes(&update_message_fixture.client_message_hex);

        let new_height = update_message_fixture.new_height;
        let (new_consensus_state_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state_pda.as_ref(),
                &new_height.to_le_bytes(),
            ],
            &crate::ID,
        );

        let update_msg = UpdateClientMsg { client_message };

        let instruction_data = crate::instruction::UpdateClient {
            msg: update_msg.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new_readonly(trusted_consensus_state_pda, false),
                AccountMeta::new(new_consensus_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        // Create new account for the new consensus state (initially empty)
        let new_consensus_account = Account {
            lamports: 0,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let mut accounts = initialized_accounts;
        accounts.push((new_consensus_state_pda, new_consensus_account));

        let mollusk = Mollusk::new(&crate::ID, "../../target/deploy/ics07_tendermint");

        let checks = vec![
            Check::success(),
            Check::account(&client_state_pda).owner(&crate::ID).build(),
            Check::account(&new_consensus_state_pda)
                .owner(&crate::ID)
                .build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        // Verify the client state was updated
        let client_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_state_pda)
            .map(|(_, account)| account)
            .expect("Client state account not found");

        let mut data_slice = &client_state_account.data[8..];
        let updated_client_state: ClientState =
            ClientState::deserialize(&mut data_slice).expect("Failed to deserialize client state");

        // The latest height should have been updated to the new height
        assert!(updated_client_state.latest_height.revision_height >= new_height);

        // Verify the new consensus state was created
        let new_consensus_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &new_consensus_state_pda)
            .map(|(_, account)| account)
            .expect("New consensus state account not found");

        assert!(
            new_consensus_state_account.lamports > 0,
            "New consensus state account should be rent-exempt"
        );
        assert!(
            new_consensus_state_account.data.len() > 8,
            "New consensus state account should have data"
        );

        let mut data_slice = &new_consensus_state_account.data[8..];
        let new_consensus_store: ConsensusStateStore =
            ConsensusStateStore::deserialize(&mut data_slice)
                .expect("Failed to deserialize new consensus state store");

        assert_eq!(new_consensus_store.height, new_height);
    }
}
