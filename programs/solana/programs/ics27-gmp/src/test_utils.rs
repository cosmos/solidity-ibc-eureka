use crate::constants::{GMP_PORT_ID, ICS27_ENCODING, ICS27_VERSION};
use crate::state::{AccountVersion, GMPAppState};
use anchor_lang::{AnchorSerialize, Discriminator, InstructionData};
use mollusk_svm::Mollusk;
use solana_sdk::{
    account::Account as SolanaAccount,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

// Anchor error code offset (all Anchor errors start from 6000)
pub const ANCHOR_ERROR_OFFSET: u32 = 6000;

// Dummy target program ID for tests that don't actually execute the target
pub const DUMMY_TARGET_PROGRAM: Pubkey = Pubkey::new_from_array([
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32,
]);

pub fn create_gmp_app_state_account(
    pubkey: Pubkey,
    authority: Pubkey,
    bump: u8,
    paused: bool,
) -> (Pubkey, SolanaAccount) {
    let app_state = GMPAppState {
        version: AccountVersion::V1,
        authority,
        paused,
        bump,
        _reserved: [0; 256],
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

pub const fn create_dummy_target_program_account() -> (Pubkey, SolanaAccount) {
    (
        DUMMY_TARGET_PROGRAM,
        SolanaAccount {
            lamports: 1_000_000,
            data: vec![],
            owner: solana_sdk::bpf_loader::ID,
            executable: true,
            rent_epoch: 0,
        },
    )
}

pub fn create_instructions_sysvar_account() -> (Pubkey, SolanaAccount) {
    create_instructions_sysvar_account_with_caller(Pubkey::new_unique())
}

pub fn create_instructions_sysvar_account_with_caller(
    caller_program_id: Pubkey,
) -> (Pubkey, SolanaAccount) {
    use solana_sdk::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };

    // Create minimal mock instructions to simulate router calling GMP via CPI
    // For CPI validation, only the program_id matters - GMP checks that
    // the calling instruction's program_id matches the authorized router
    //
    // Instruction 0: The router/caller instruction (current when router executes)
    // During CPI, current_index points to this instruction
    let account_pubkey = Pubkey::new_unique();
    let account = BorrowedAccountMeta {
        pubkey: &account_pubkey,
        is_signer: false,
        is_writable: true,
    };
    let mock_caller_ix = BorrowedInstruction {
        program_id: &caller_program_id,
        accounts: vec![account],
        data: &[],
    };

    // Serialize instructions for sysvar
    // When GMP checks the sysvar during CPI, it'll see the caller as the executing instruction
    let ixs_data = construct_instructions_data(&[mock_caller_ix]);

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

/// Creates a fake instructions sysvar account with a different pubkey than the real one
/// This simulates the Wormhole-style attack where an attacker passes a fake sysvar
pub fn create_fake_instructions_sysvar_account(
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
    let mock_caller_ix = BorrowedInstruction {
        program_id: &caller_program_id,
        accounts: vec![account],
        data: &[],
    };

    let ixs_data = construct_instructions_data(&[mock_caller_ix]);

    // Use a FAKE pubkey (not the real instructions sysvar ID)
    let fake_sysvar_pubkey = Pubkey::new_unique();

    (
        fake_sysvar_pubkey,
        SolanaAccount {
            lamports: 1_000_000,
            data: ixs_data,
            owner: solana_sdk::sysvar::ID,
            executable: false,
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

/// Helper to create common test data: (`client_id`, `sender`, `salt`, `account_state_pda`)
pub fn create_test_account_data() -> (&'static str, &'static str, Vec<u8>, Pubkey) {
    let client_id = "cosmoshub-1";
    let sender = "cosmos1test";
    let salt = vec![1u8, 2, 3];

    let (account_state_pda, _) = solana_ibc_types::GMPAccount::new(
        solana_ibc_types::ClientId::new(client_id).unwrap(),
        solana_ibc_types::Sender::new(sender).unwrap(),
        solana_ibc_types::Salt::new(salt.clone()).unwrap(),
        &crate::ID,
    )
    .pda();

    (client_id, sender, salt, account_state_pda)
}

pub fn create_gmp_test_context() -> GmpTestContext {
    let authority = Pubkey::new_unique();
    let payer = Pubkey::new_unique();
    let (app_state_pda, app_state_bump) =
        Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

    GmpTestContext {
        mollusk: Mollusk::new(&crate::ID, crate::get_gmp_program_path()),
        authority,
        router_program: ics26_router::ID,
        payer,
        app_state_pda,
        app_state_bump,
    }
}

/// Create GMP packet data (proto wire format - no `client_id`)
/// The `client_id` comes from IBC context (`OnRecvPacketMsg.source_client`), not the packet payload
pub fn create_gmp_packet_data(
    sender: &str,
    receiver: &str,
    salt: Vec<u8>,
    payload: Vec<u8>,
) -> crate::proto::GmpPacketData {
    crate::proto::GmpPacketData {
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
    payer: Pubkey,
    msg: solana_ibc_types::OnRecvPacketMsg,
) -> Instruction {
    let instruction_data = crate::instruction::OnRecvPacket { msg };

    Instruction {
        program_id: crate::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: instruction_data.data(),
    }
}
