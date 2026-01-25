//! Test utilities for ICS27 IFT program
//!
//! Provides helper functions for mollusk-based unit tests.

use anchor_lang::prelude::*;
use mollusk_svm::Mollusk;
use solana_sdk::{account::Account as SolanaAccount, pubkey::Pubkey, system_program};

use crate::constants::*;
use crate::state::{
    AccountVersion, CounterpartyChainType, IFTAppState, IFTBridge, PendingTransfer,
};

/// Path to the compiled IFT program binary
pub const IFT_PROGRAM_PATH: &str = "../../target/deploy/ics27_ift";

/// Anchor error code offset
pub const ANCHOR_ERROR_OFFSET: u32 = 6000;

/// Setup mollusk with the IFT program
pub fn setup_mollusk() -> Mollusk {
    Mollusk::new(&crate::ID, IFT_PROGRAM_PATH)
}

/// Create a serialized IFT app state account
pub fn create_ift_app_state_account(
    mint: Pubkey,
    bump: u8,
    mint_authority_bump: u8,
    access_manager: Pubkey,
    gmp_program: Pubkey,
) -> SolanaAccount {
    let app_state = IFTAppState {
        version: AccountVersion::V1,
        bump,
        mint,
        mint_authority_bump,
        access_manager,
        gmp_program,
        _reserved: [0; 128],
    };

    let mut data = IFTAppState::DISCRIMINATOR.to_vec();
    app_state.serialize(&mut data).unwrap();

    SolanaAccount {
        lamports: 1_000_000,
        data,
        owner: crate::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Create a serialized IFT bridge account
pub fn create_ift_bridge_account(
    mint: Pubkey,
    client_id: &str,
    counterparty_ift_address: &str,
    counterparty_chain_type: CounterpartyChainType,
    bump: u8,
    active: bool,
) -> SolanaAccount {
    let bridge = IFTBridge {
        version: AccountVersion::V1,
        bump,
        mint,
        client_id: client_id.to_string(),
        counterparty_ift_address: counterparty_ift_address.to_string(),
        counterparty_chain_type,
        active,
        _reserved: [0; 64],
    };

    let mut data = IFTBridge::DISCRIMINATOR.to_vec();
    bridge.serialize(&mut data).unwrap();

    SolanaAccount {
        lamports: 1_000_000,
        data,
        owner: crate::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Create a serialized pending transfer account
pub fn create_pending_transfer_account(
    mint: Pubkey,
    client_id: &str,
    sequence: u64,
    sender: Pubkey,
    amount: u64,
    bump: u8,
) -> SolanaAccount {
    let pending = PendingTransfer {
        version: AccountVersion::V1,
        bump,
        mint,
        client_id: client_id.to_string(),
        sequence,
        sender,
        amount,
        timestamp: 1_700_000_000,
        _reserved: [0; 32],
    };

    let mut data = PendingTransfer::DISCRIMINATOR.to_vec();
    pending.serialize(&mut data).unwrap();

    SolanaAccount {
        lamports: 1_000_000,
        data,
        owner: crate::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Create a signer account with lamports
pub fn create_signer_account() -> SolanaAccount {
    SolanaAccount {
        lamports: 1_000_000_000,
        data: vec![],
        owner: system_program::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Create an uninitialized PDA account (for account init)
pub fn create_uninitialized_pda() -> SolanaAccount {
    SolanaAccount {
        lamports: 0,
        data: vec![],
        owner: system_program::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Create system program account
pub fn create_system_program_account() -> (Pubkey, SolanaAccount) {
    (
        system_program::ID,
        SolanaAccount {
            lamports: 1,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        },
    )
}

/// Create instructions sysvar account for direct call
pub fn create_instructions_sysvar_account() -> (Pubkey, SolanaAccount) {
    create_instructions_sysvar_account_with_caller(crate::ID)
}

/// Create instructions sysvar account with specific caller program ID
pub fn create_instructions_sysvar_account_with_caller(
    caller_program_id: Pubkey,
) -> (Pubkey, SolanaAccount) {
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
        SolanaAccount {
            lamports: 1_000_000,
            data: ixs_data,
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

/// Create access manager account with admin role for a user
pub fn create_access_manager_account_with_admin(admin: Pubkey) -> (Pubkey, SolanaAccount) {
    use access_manager::state::AccessManager;
    use solana_ibc_types::roles;

    let (access_manager_pda, _) =
        Pubkey::find_program_address(&[AccessManager::SEED], &access_manager::ID);

    let access_manager = AccessManager {
        roles: vec![access_manager::types::RoleData {
            role_id: roles::ADMIN_ROLE,
            members: vec![admin],
        }],
    };

    let mut data = vec![0u8; 8 + AccessManager::INIT_SPACE];
    data[0..8].copy_from_slice(AccessManager::DISCRIMINATOR);
    access_manager.serialize(&mut &mut data[8..]).unwrap();

    (
        access_manager_pda,
        SolanaAccount {
            lamports: 1_000_000,
            data,
            owner: access_manager::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

/// Create clock sysvar account
pub fn create_clock_sysvar_account(unix_timestamp: i64) -> (Pubkey, SolanaAccount) {
    let clock = solana_sdk::sysvar::clock::Clock {
        slot: 1000,
        epoch_start_timestamp: 0,
        epoch: 1,
        leader_schedule_epoch: 1,
        unix_timestamp,
    };

    (
        solana_sdk::sysvar::clock::ID,
        SolanaAccount {
            lamports: 1,
            data: bincode::serialize(&clock).expect("Failed to serialize Clock sysvar"),
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

/// Get the IFT app state PDA
pub fn get_app_state_pda(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[IFT_APP_STATE_SEED, mint.as_ref()], &crate::ID)
}

/// Get the IFT bridge PDA
pub fn get_bridge_pda(mint: &Pubkey, client_id: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[IFT_BRIDGE_SEED, mint.as_ref(), client_id.as_bytes()],
        &crate::ID,
    )
}

/// Get the pending transfer PDA
pub fn get_pending_transfer_pda(mint: &Pubkey, client_id: &str, sequence: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[
            PENDING_TRANSFER_SEED,
            mint.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &crate::ID,
    )
}

/// Get the mint authority PDA
pub fn get_mint_authority_pda(mint: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[MINT_AUTHORITY_SEED, mint.as_ref()], &crate::ID)
}

/// Deserialize IFT app state from account
pub fn deserialize_app_state(account: &SolanaAccount) -> IFTAppState {
    anchor_lang::AccountDeserialize::try_deserialize(&mut &account.data[..])
        .expect("Failed to deserialize IFTAppState")
}

/// Deserialize IFT bridge from account
pub fn deserialize_bridge(account: &SolanaAccount) -> IFTBridge {
    anchor_lang::AccountDeserialize::try_deserialize(&mut &account.data[..])
        .expect("Failed to deserialize IFTBridge")
}

pub fn get_gmp_account_pda(
    client_id: &str,
    counterparty_address: &str,
    gmp_program: &Pubkey,
) -> (Pubkey, u8) {
    use solana_ibc_types::ics27::{GMPAccount, Salt};

    let gmp_account = GMPAccount::new(
        client_id.to_string().try_into().expect("valid client_id"),
        counterparty_address
            .to_string()
            .try_into()
            .expect("valid sender"),
        Salt::empty(),
        gmp_program,
    );
    gmp_account.pda()
}

/// Create a GMP program account
pub fn create_gmp_program_account() -> SolanaAccount {
    SolanaAccount {
        lamports: 1,
        data: vec![],
        owner: solana_sdk::native_loader::ID,
        executable: true,
        rent_epoch: 0,
    }
}
