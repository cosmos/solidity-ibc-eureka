use crate::state::AccessManager;
use crate::types::RoleData;
use anchor_lang::prelude::*;
use mollusk_svm::{result::InstructionResult, Mollusk};
use solana_ibc_types::roles;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

pub fn serialize_access_manager(access_manager: &AccessManager) -> Vec<u8> {
    let mut data = AccessManager::DISCRIMINATOR.to_vec();
    access_manager.serialize(&mut data).unwrap();
    data
}

pub fn create_initialized_access_manager(admin: Pubkey) -> (Pubkey, Account) {
    let (access_manager_pda, _) = Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

    let access_manager = AccessManager {
        roles: vec![RoleData {
            role_id: roles::ADMIN_ROLE,
            members: vec![admin],
        }],
    };

    // Use INIT_SPACE to ensure account has enough space for max roles
    let mut data = vec![0u8; 8 + AccessManager::INIT_SPACE];
    data[0..8].copy_from_slice(AccessManager::DISCRIMINATOR);
    access_manager.serialize(&mut &mut data[8..]).unwrap();

    (
        access_manager_pda,
        Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_access_manager_with_role(
    admin: Pubkey,
    role_id: u64,
    member: Pubkey,
) -> (Pubkey, Account) {
    let (access_manager_pda, _) = Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

    let mut roles_vec = vec![RoleData {
        role_id: roles::ADMIN_ROLE,
        members: vec![admin],
    }];

    // Add the requested role if it's not already ADMIN_ROLE or if member is different
    if role_id != roles::ADMIN_ROLE || member != admin {
        if let Some(existing_role) = roles_vec.iter_mut().find(|r| r.role_id == role_id) {
            if !existing_role.members.contains(&member) {
                existing_role.members.push(member);
            }
        } else {
            roles_vec.push(RoleData {
                role_id,
                members: vec![member],
            });
        }
    }

    let access_manager = AccessManager { roles: roles_vec };

    // Use INIT_SPACE to ensure account has enough space for max roles
    let mut data = vec![0u8; 8 + AccessManager::INIT_SPACE];
    data[0..8].copy_from_slice(AccessManager::DISCRIMINATOR);
    access_manager.serialize(&mut &mut data[8..]).unwrap();

    (
        access_manager_pda,
        Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_signer_account() -> Account {
    Account {
        lamports: 1_000_000_000,
        data: vec![],
        owner: system_program::ID,
        executable: false,
        rent_epoch: 0,
    }
}

pub fn setup_mollusk() -> Mollusk {
    Mollusk::new(&crate::ID, crate::get_access_manager_program_path())
}

/// Anchor error code offset
pub const ANCHOR_ERROR_OFFSET: u32 = 6000;

/// Build instruction for access-manager program
pub fn build_instruction<T: anchor_lang::InstructionData>(
    instruction_data: T,
    accounts: Vec<anchor_lang::prelude::AccountMeta>,
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts,
        data: instruction_data.data(),
    }
}

/// Get the `access_manager` PDA
pub fn get_access_manager_pda() -> Pubkey {
    Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID).0
}

/// Deserialize `access_manager` from instruction result
pub fn get_access_manager_from_result(result: &InstructionResult, pda: &Pubkey) -> AccessManager {
    let account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| pubkey == pda)
        .map(|(_, account)| account)
        .expect("Access manager account not found");

    anchor_lang::AccountDeserialize::try_deserialize(&mut &account.data[..])
        .expect("Failed to deserialize access manager")
}

/// Create instructions sysvar account for direct call (not CPI)
pub fn create_instructions_sysvar_account() -> Account {
    use solana_sdk::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };

    // Create minimal mock instruction to simulate direct call
    // Current instruction has this program as the program_id
    let account_pubkey = Pubkey::new_unique();
    let account = BorrowedAccountMeta {
        pubkey: &account_pubkey,
        is_signer: false,
        is_writable: true,
    };
    let mock_instruction = BorrowedInstruction {
        program_id: &crate::ID, // Direct call to our program
        accounts: vec![account],
        data: &[],
    };

    let ixs_data = construct_instructions_data(&[mock_instruction]);

    Account {
        lamports: 1_000_000,
        data: ixs_data,
        owner: solana_sdk::sysvar::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Create fake instructions sysvar for testing Wormhole-style attack
/// Returns (`fake_pubkey`, account) where `fake_pubkey` is NOT the real instructions sysvar ID
pub fn create_fake_instructions_sysvar_account(caller_program_id: Pubkey) -> (Pubkey, Account) {
    use solana_sdk::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };

    let account_pubkey = Pubkey::new_unique();
    let account = BorrowedAccountMeta {
        pubkey: &account_pubkey,
        is_signer: false,
        is_writable: true,
    };
    let mock_instruction = BorrowedInstruction {
        program_id: &caller_program_id, // Fake caller program
        accounts: vec![account],
        data: &[],
    };

    let ixs_data = construct_instructions_data(&[mock_instruction]);

    // Use a FAKE pubkey (not the real instructions sysvar ID)
    let fake_sysvar_pubkey = Pubkey::new_unique();

    (
        fake_sysvar_pubkey,
        Account {
            lamports: 1_000_000,
            data: ixs_data,
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

/// Helper for testing Wormhole-style fake sysvar attacks
/// Automatically finds and replaces the instructions sysvar with a fake one
/// Returns (`modified_instruction`, `fake_sysvar_account_tuple`)
pub fn setup_fake_sysvar_attack(
    mut instruction: Instruction,
    program_id: Pubkey,
) -> (Instruction, (Pubkey, Account)) {
    let (fake_sysvar_pubkey, fake_sysvar_account) =
        create_fake_instructions_sysvar_account(program_id);

    // Find the instructions sysvar account and replace it with the fake one
    let sysvar_account_index = instruction
        .accounts
        .iter()
        .position(|acc| acc.pubkey == solana_sdk::sysvar::instructions::ID)
        .expect("Instructions sysvar account not found in instruction");

    instruction.accounts[sysvar_account_index] =
        AccountMeta::new_readonly(fake_sysvar_pubkey, false);

    (instruction, (fake_sysvar_pubkey, fake_sysvar_account))
}

/// Expected error for Wormhole-style sysvar attacks (Anchor's address constraint violation)
pub fn expect_sysvar_attack_error() -> mollusk_svm::result::Check<'static> {
    mollusk_svm::result::Check::err(solana_sdk::program_error::ProgramError::Custom(
        anchor_lang::error::ErrorCode::ConstraintAddress as u32,
    ))
}

/// Create instructions sysvar that simulates a CPI call from another program
/// Uses the REAL sysvar address but with a different `program_id` to simulate CPI context
pub fn create_cpi_instructions_sysvar_account(caller_program_id: Pubkey) -> Account {
    use solana_sdk::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };

    let account_pubkey = Pubkey::new_unique();
    let account = BorrowedAccountMeta {
        pubkey: &account_pubkey,
        is_signer: false,
        is_writable: true,
    };
    let mock_instruction = BorrowedInstruction {
        program_id: &caller_program_id, // Different program calling via CPI
        accounts: vec![account],
        data: &[],
    };

    let ixs_data = construct_instructions_data(&[mock_instruction]);

    Account {
        lamports: 1_000_000,
        data: ixs_data,
        owner: solana_sdk::sysvar::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Helper for testing CPI rejection
/// Replaces the instructions sysvar with one that simulates a CPI call
/// Returns (`modified_instruction`, `cpi_sysvar_account_tuple`)
pub fn setup_cpi_call_test(
    instruction: Instruction,
    caller_program_id: Pubkey,
) -> (Instruction, (Pubkey, Account)) {
    let cpi_sysvar_account = create_cpi_instructions_sysvar_account(caller_program_id);

    // Use the REAL sysvar address (unlike Wormhole attack which uses fake)
    (
        instruction,
        (solana_sdk::sysvar::instructions::ID, cpi_sysvar_account),
    )
}

/// Expected error for CPI rejection (`UnauthorizedCaller` from `reject_cpi`)
/// This is for instructions that DON'T map the error (like router/gmp callback instructions)
pub fn expect_cpi_rejection_error() -> mollusk_svm::result::Check<'static> {
    use solana_ibc_types::CpiValidationError;
    mollusk_svm::result::Check::err(solana_sdk::program_error::ProgramError::Custom(
        anchor_lang::error::ERROR_CODE_OFFSET + CpiValidationError::UnauthorizedCaller as u32,
    ))
}

/// Expected error for CPI rejection in access-manager instructions
/// These instructions map `CpiValidationError::UnauthorizedCaller` to `AccessManagerError::CpiNotAllowed`
pub fn expect_access_manager_cpi_rejection_error() -> mollusk_svm::result::Check<'static> {
    use crate::errors::AccessManagerError;
    mollusk_svm::result::Check::err(solana_sdk::program_error::ProgramError::Custom(
        ANCHOR_ERROR_OFFSET + AccessManagerError::CpiNotAllowed as u32,
    ))
}

/// Create instructions sysvar account for direct call (alias for `create_instructions_sysvar_account`)
pub fn create_instructions_sysvar_account_with_caller(
    caller_program_id: Pubkey,
) -> (Pubkey, Account) {
    use solana_sdk::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };

    let account_pubkey = Pubkey::new_unique();
    let account = BorrowedAccountMeta {
        pubkey: &account_pubkey,
        is_signer: false,
        is_writable: true,
    };
    let mock_instruction = BorrowedInstruction {
        program_id: &caller_program_id,
        accounts: vec![account],
        data: &[],
    };

    let ixs_data = construct_instructions_data(&[mock_instruction]);

    (
        solana_sdk::sysvar::instructions::ID,
        Account {
            lamports: 1_000_000,
            data: ixs_data,
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

/// Deserialize account data into typed struct
pub fn get_account_data<T: anchor_lang::AccountDeserialize>(account: &Account) -> T {
    anchor_lang::AccountDeserialize::try_deserialize(&mut &account.data[..])
        .expect("Failed to deserialize account")
}

/// Serialize account struct to data
pub fn serialize_account<T: anchor_lang::Discriminator + anchor_lang::AnchorSerialize>(
    account: &T,
) -> Vec<u8> {
    let mut data = T::DISCRIMINATOR.to_vec();
    account.serialize(&mut data).unwrap();
    data
}

/// Create rent sysvar account
pub fn create_rent_sysvar_account() -> (Pubkey, Account) {
    (
        solana_sdk::sysvar::rent::ID,
        Account {
            lamports: 1_000_000,
            data: vec![0; 17], // Rent sysvar is 17 bytes
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

/// Create clock sysvar account
pub fn create_clock_sysvar_account() -> (Pubkey, Account) {
    (
        solana_sdk::sysvar::clock::ID,
        Account {
            lamports: 1_000_000,
            data: vec![0; 40], // Clock sysvar is 40 bytes
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}
