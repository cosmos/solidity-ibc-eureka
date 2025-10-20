use crate::state::{AccountState, GMPAppState};
use anchor_lang::{AnchorSerialize, Discriminator};
use solana_sdk::{account::Account as SolanaAccount, pubkey::Pubkey, system_program};

pub fn create_gmp_app_state_account(
    pubkey: Pubkey,
    router_program: Pubkey,
    authority: Pubkey,
    bump: u8,
    paused: bool,
) -> (Pubkey, SolanaAccount) {
    let app_state = GMPAppState {
        router_program,
        authority,
        version: 1,
        paused,
        bump,
    };

    let mut data = Vec::new();
    data.extend_from_slice(GMPAppState::DISCRIMINATOR);
    app_state.serialize(&mut data).unwrap();

    (
        pubkey,
        SolanaAccount {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_account_state(
    pubkey: Pubkey,
    client_id: String,
    sender: String,
    salt: Vec<u8>,
    bump: u8,
) -> (Pubkey, SolanaAccount) {
    let account_state = AccountState {
        client_id,
        sender,
        salt,
        nonce: 0,
        created_at: 1_600_000_000,
        last_executed_at: 1_600_000_000,
        execution_count: 0,
        bump,
    };

    let mut data = Vec::new();
    data.extend_from_slice(AccountState::DISCRIMINATOR);
    account_state.serialize(&mut data).unwrap();

    (
        pubkey,
        SolanaAccount {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_account_state_with_nonce(
    pubkey: Pubkey,
    client_id: String,
    sender: String,
    salt: Vec<u8>,
    nonce: u64,
    bump: u8,
) -> (Pubkey, SolanaAccount) {
    let account_state = AccountState {
        client_id,
        sender,
        salt,
        nonce,
        created_at: 1_600_000_000,
        last_executed_at: 1_600_000_000,
        execution_count: 0,
        bump,
    };

    let mut data = Vec::new();
    data.extend_from_slice(AccountState::DISCRIMINATOR);
    account_state.serialize(&mut data).unwrap();

    (
        pubkey,
        SolanaAccount {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub const fn create_authority_account(pubkey: Pubkey) -> (Pubkey, SolanaAccount) {
    (
        pubkey,
        SolanaAccount {
            lamports: 1_000_000_000,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub const fn create_router_program_account(pubkey: Pubkey) -> (Pubkey, SolanaAccount) {
    (
        pubkey,
        SolanaAccount {
            lamports: 1_000_000,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        },
    )
}

pub const fn create_pda_for_init(pubkey: Pubkey) -> (Pubkey, SolanaAccount) {
    (
        pubkey,
        SolanaAccount {
            lamports: 0,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub const fn create_payer_account(pubkey: Pubkey) -> (Pubkey, SolanaAccount) {
    (
        pubkey,
        SolanaAccount {
            lamports: 1_000_000_000,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub const fn create_system_program_account() -> (Pubkey, SolanaAccount) {
    (
        system_program::ID,
        SolanaAccount {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        },
    )
}

pub const fn create_uninitialized_account_for_pda(pubkey: Pubkey) -> (Pubkey, SolanaAccount) {
    (
        pubkey,
        SolanaAccount {
            lamports: 0,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}
