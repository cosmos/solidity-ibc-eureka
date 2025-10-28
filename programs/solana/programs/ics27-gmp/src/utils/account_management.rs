use crate::errors::GMPError;
use crate::state::AccountState;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{hash::hash, program::invoke_signed, system_instruction};
use anchor_lang::Discriminator;

const ADDRESS_HASH_PREFIX: &[u8] = b"gmp_sender_address";

/// Get or create account state with proper PDA validation
#[allow(clippy::too_many_arguments)]
pub fn get_or_create_account<'a>(
    account_info: &AccountInfo<'a>,
    client_id: &str,
    sender: &str,
    salt: &[u8],
    payer: &AccountInfo<'a>,
    _system_program: &AccountInfo<'a>,
    program_id: &Pubkey,
    current_time: i64,
    expected_bump: u8,
) -> Result<(AccountState, bool)> {
    // Derive expected address and validate
    let (expected_address, derived_bump) =
        AccountState::derive_address(client_id, sender, salt, program_id)?;

    require!(
        account_info.key() == expected_address,
        GMPError::InvalidAccountAddress
    );

    require!(
        expected_bump == derived_bump,
        GMPError::InvalidAccountAddress
    );

    // Check if account already exists
    if account_info.data_is_empty() {
        // Create new account
        let account_size = crate::constants::DISCRIMINATOR_SIZE + AccountState::INIT_SPACE;
        let rent = Rent::get()?;
        let required_lamports = rent.minimum_balance(account_size);

        // Create account via system program using invoke_signed
        // The account_info is a PDA, so we need to sign for it
        let sender_hash = hash(sender.as_bytes()).to_bytes();
        let signer_seeds: &[&[u8]] = &[
            AccountState::SEED,
            client_id.as_bytes(),
            &sender_hash,
            salt,
            &[expected_bump],
        ];

        invoke_signed(
            &system_instruction::create_account(
                payer.key,
                account_info.key,
                required_lamports,
                account_size as u64,
                program_id,
            ),
            &[payer.clone(), account_info.clone()],
            &[signer_seeds],
        )?;

        // Initialize account state
        let account_state = AccountState {
            client_id: client_id.to_string(),
            sender: sender.to_string(),
            salt: salt.to_vec(),
            nonce: 0,
            created_at: current_time,
            last_executed_at: 0,
            execution_count: 0,
            bump: expected_bump,
        };

        // Serialize and write the account state
        save_account_state(account_info, &account_state)?;

        Ok((account_state, true))
    } else {
        // Load existing account
        let account_data = account_info.try_borrow_data()?;

        // Skip discriminator (8 bytes) and deserialize
        let account_state = AccountState::try_deserialize(
            &mut &account_data[crate::constants::DISCRIMINATOR_SIZE..],
        )?;

        Ok((account_state, false))
    }
}

/// Save account state to account data
pub fn save_account_state(
    account_info: &AccountInfo<'_>,
    account_state: &AccountState,
) -> Result<()> {
    let mut account_data = account_info.try_borrow_mut_data()?;

    // Write discriminator
    account_data[0..crate::constants::DISCRIMINATOR_SIZE]
        .copy_from_slice(AccountState::DISCRIMINATOR);

    // Serialize account state after discriminator
    account_state.try_serialize(&mut &mut account_data[crate::constants::DISCRIMINATOR_SIZE..])?;

    Ok(())
}

/// Validate account ownership and program
pub fn validate_account_ownership(
    account_info: &AccountInfo<'_>,
    program_id: &Pubkey,
) -> Result<()> {
    require!(
        account_info.owner == program_id,
        GMPError::InvalidAccountAddress
    );
    Ok(())
}

/// Calculate account rent requirement
pub fn calculate_account_rent() -> Result<u64> {
    let account_size = crate::constants::DISCRIMINATOR_SIZE + AccountState::INIT_SPACE;
    let rent = Rent::get()?;
    Ok(rent.minimum_balance(account_size))
}

/// Convert any cross-chain address string to a deterministic Solana Pubkey
/// Uses hashing to ensure the same address always maps to the same Pubkey
pub fn derive_pubkey_from_address(sender_address: &str) -> Result<Pubkey> {
    // Validate input is not empty
    if sender_address.is_empty() {
        return Err(GMPError::InvalidPacketData.into());
    }

    // Use deterministic hash-based derivation for any address format
    // This is simple, reliable, and works for all blockchain address formats
    let mut seed_data = Vec::new();
    seed_data.extend_from_slice(ADDRESS_HASH_PREFIX);
    seed_data.extend_from_slice(sender_address.as_bytes());

    let hash_result = hash(&seed_data);
    Ok(Pubkey::new_from_array(hash_result.to_bytes()))
}
