use crate::constants::{GMP_PORT_ID, ICS27_ENCODING, ICS27_VERSION};
use crate::state::{AccountState, GMPAppState, GMPPacketData};
use anchor_lang::{AnchorSerialize, Discriminator, InstructionData};
use mollusk_svm::Mollusk;
use solana_sdk::{
    account::Account as SolanaAccount,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

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

pub struct GmpTestContext {
    pub mollusk: Mollusk,
    pub authority: Pubkey,
    pub router_program: Pubkey,
    pub payer: Pubkey,
    pub app_state_pda: Pubkey,
    pub app_state_bump: u8,
}

pub fn create_gmp_test_context() -> GmpTestContext {
    let authority = Pubkey::new_unique();
    let router_program = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) = Pubkey::find_program_address(
        &[crate::constants::GMP_APP_STATE_SEED, GMP_PORT_ID.as_bytes()],
        &crate::ID,
    );

    GmpTestContext {
        mollusk: Mollusk::new(&crate::ID, crate::get_gmp_program_path()),
        authority,
        router_program,
        payer,
        app_state_pda,
        app_state_bump,
    }
}

pub fn create_gmp_packet_data(
    client_id: &str,
    sender: &str,
    receiver: Pubkey,
    salt: Vec<u8>,
    payload: Vec<u8>,
) -> GMPPacketData {
    GMPPacketData {
        client_id: client_id.to_string(),
        sender: sender.to_string(),
        receiver: receiver.to_string(),
        salt,
        payload,
        memo: String::new(),
    }
}

pub fn create_recv_packet_msg(
    client_id: &str,
    packet_data_bytes: Vec<u8>,
    sequence: u64,
) -> solana_ibc_types::OnRecvPacketMsg {
    solana_ibc_types::OnRecvPacketMsg {
        source_client: client_id.to_string(),
        dest_client: "solana-1".to_string(),
        sequence,
        payload: solana_ibc_types::Payload {
            source_port: GMP_PORT_ID.to_string(),
            dest_port: GMP_PORT_ID.to_string(),
            version: ICS27_VERSION.to_string(),
            encoding: ICS27_ENCODING.to_string(),
            value: packet_data_bytes,
        },
        relayer: Pubkey::new_unique(),
    }
}

pub fn create_recv_packet_instruction(
    app_state_pda: Pubkey,
    router_program: Pubkey,
    payer: Pubkey,
    msg: solana_ibc_types::OnRecvPacketMsg,
) -> Instruction {
    let instruction_data = crate::instruction::OnRecvPacket { msg };

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(router_program, false),
            AccountMeta::new(payer, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: instruction_data.data(),
    }
}
