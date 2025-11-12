use crate::state::AccessManager;
use crate::types::{AccessManagerVersion, RoleData};
use anchor_lang::prelude::*;
use mollusk_svm::{result::InstructionResult, Mollusk};
use solana_sdk::{account::Account, instruction::Instruction, pubkey::Pubkey, system_program};

pub fn serialize_access_manager(access_manager: &AccessManager) -> Vec<u8> {
    let mut data = AccessManager::DISCRIMINATOR.to_vec();
    access_manager.serialize(&mut data).unwrap();
    data
}

pub fn create_initialized_access_manager(admin: Pubkey) -> (Pubkey, Account) {
    let (access_manager_pda, _) = Pubkey::find_program_address(&[AccessManager::SEED], &crate::ID);

    let access_manager = AccessManager {
        version: AccessManagerVersion::V1,
        admin,
        roles: vec![],
        _reserved: [0; 256],
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

    let access_manager = AccessManager {
        version: AccessManagerVersion::V1,
        admin,
        roles: vec![RoleData {
            role_id,
            members: vec![member],
        }],
        _reserved: [0; 256],
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
