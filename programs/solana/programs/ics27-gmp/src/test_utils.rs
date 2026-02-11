use crate::constants::{GMP_PORT_ID, ICS27_ENCODING, ICS27_VERSION};
use crate::proto::RawGmpPacketData;
use crate::state::{AccountVersion, GMPAppState};
use access_manager::RoleData;
use anchor_lang::{AnchorSerialize, Discriminator, InstructionData};
use mollusk_svm::Mollusk;
use solana_ibc_types::roles;
use solana_sdk::{
    account::Account as SolanaAccount,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signer::Signer,
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
    bump: u8,
    paused: bool,
) -> (Pubkey, SolanaAccount) {
    let app_state = GMPAppState {
        version: AccountVersion::V1,
        paused,
        bump,
        access_manager: access_manager::ID,
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

pub fn setup_access_manager_with_roles(roles: &[(u64, &[Pubkey])]) -> (Pubkey, SolanaAccount) {
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);

    let mut role_data: Vec<RoleData> = roles
        .iter()
        .map(|(role_id, members)| RoleData {
            role_id: *role_id,
            members: members.to_vec(),
        })
        .collect();

    // Ensure ADMIN_ROLE exists with at least one member
    if !role_data.iter().any(|r| r.role_id == roles::ADMIN_ROLE) {
        role_data.push(RoleData {
            role_id: roles::ADMIN_ROLE,
            members: vec![Pubkey::new_unique()],
        });
    }

    let access_manager = access_manager::state::AccessManager { roles: role_data };

    let mut data = access_manager::state::AccessManager::DISCRIMINATOR.to_vec();
    access_manager.serialize(&mut data).unwrap();

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

/// Helper to create common test data: (`client_id`, `sender`, `salt`, `gmp_account_pda`)
pub fn create_test_account_data() -> (&'static str, &'static str, Vec<u8>, Pubkey) {
    let client_id = "cosmoshub-1";
    let sender = "cosmos1test";
    let salt = vec![1u8, 2, 3];

    let (gmp_account_pda, _) = solana_ibc_types::GMPAccount::new(
        client_id.try_into().unwrap(),
        sender.try_into().unwrap(),
        salt.clone().try_into().unwrap(),
        &crate::ID,
    )
    .pda();

    (client_id, sender, salt, gmp_account_pda)
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
/// The `client_id` comes from IBC context (`OnRecvPacketMsg.dest_client`), not the packet payload
pub fn create_gmp_packet_data(
    sender: &str,
    receiver: &str,
    salt: Vec<u8>,
    payload: Vec<u8>,
) -> RawGmpPacketData {
    RawGmpPacketData {
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
        source_client: "cosmos-1".to_string(),
        dest_client: client_id.to_string(),
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

/// Create initialized app state for tests
pub fn create_initialized_app_state(_access_manager_program_id: Pubkey) -> (Pubkey, SolanaAccount) {
    let (app_state_pda, bump) =
        Pubkey::find_program_address(&[GMPAppState::SEED, GMP_PORT_ID.as_bytes()], &crate::ID);

    create_gmp_app_state_account(app_state_pda, bump, false)
}

/// Create access manager with a specific role
pub fn create_access_manager_with_role(
    admin: Pubkey,
    role_id: u64,
    member: Pubkey,
) -> (Pubkey, SolanaAccount) {
    let admin_members = [admin];
    let role_members = [member];

    let roles: &[(u64, &[Pubkey])] = if role_id == roles::ADMIN_ROLE && member == admin {
        &[(role_id, &role_members[..])]
    } else {
        &[
            (roles::ADMIN_ROLE, &admin_members[..]),
            (role_id, &role_members[..]),
        ]
    };

    setup_access_manager_with_roles(roles)
}

/// Build instruction for GMP program
pub fn build_instruction<T: InstructionData>(
    instruction_data: T,
    accounts: Vec<AccountMeta>,
) -> Instruction {
    Instruction {
        program_id: crate::ID,
        accounts,
        data: instruction_data.data(),
    }
}

/// Create signer account for tests
pub fn create_signer_account() -> SolanaAccount {
    SolanaAccount {
        lamports: 1_000_000_000,
        data: vec![],
        owner: system_program::ID,
        executable: false,
        rent_epoch: 0,
    }
}

/// Setup mollusk for tests
pub fn setup_mollusk() -> Mollusk {
    Mollusk::new(&crate::ID, crate::get_gmp_program_path())
}

/// Get app state from mollusk instruction result
pub fn get_app_state_from_result(
    result: &mollusk_svm::result::InstructionResult,
    pda: &Pubkey,
) -> GMPAppState {
    use anchor_lang::AccountDeserialize;

    let account = result
        .resulting_accounts
        .iter()
        .find(|(pubkey, _)| pubkey == pda)
        .map(|(_, account)| account)
        .expect("App state account not found");

    GMPAppState::try_deserialize(&mut &account.data[..]).expect("Failed to deserialize app state")
}

/// Helper for testing Wormhole-style fake sysvar attacks
/// Automatically finds and replaces the instructions sysvar with a fake one
/// Returns (`modified_instruction`, `fake_sysvar_account_tuple`)
pub fn setup_fake_sysvar_attack(
    mut instruction: Instruction,
    program_id: Pubkey,
) -> (Instruction, (Pubkey, SolanaAccount)) {
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
pub fn create_cpi_instructions_sysvar_account(caller_program_id: Pubkey) -> SolanaAccount {
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

    SolanaAccount {
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
) -> (Instruction, (Pubkey, SolanaAccount)) {
    let cpi_sysvar_account = create_cpi_instructions_sysvar_account(caller_program_id);

    // Use the REAL sysvar address (unlike Wormhole attack which uses fake)
    (
        instruction,
        (solana_sdk::sysvar::instructions::ID, cpi_sysvar_account),
    )
}

/// Expected error for CPI rejection (`UnauthorizedCaller` from `reject_cpi`)
pub fn expect_cpi_rejection_error() -> mollusk_svm::result::Check<'static> {
    use solana_ibc_types::CpiValidationError;
    mollusk_svm::result::Check::err(solana_sdk::program_error::ProgramError::Custom(
        anchor_lang::error::ERROR_CODE_OFFSET + CpiValidationError::UnauthorizedCaller as u32,
    ))
}

// ── ProgramTest (BPF runtime) integration test helpers ──

pub const MALICIOUS_CALLER_ID: Pubkey =
    solana_sdk::pubkey!("CtQLLKbDMt1XVNXtLKJEt1K8cstbckjqE6zyFqR37KTc");
pub const CPI_TEST_TARGET_ID: Pubkey =
    solana_sdk::pubkey!("HjJW8tAcq7PeaRDTR8bx22HPoh1AvLyNuKZtkgyk4i5n");
const DEPLOY_DIR: &str = "../../target/deploy";

pub const DIRECT_CALL_NOT_ALLOWED_ERROR: u32 =
    anchor_lang::error::ERROR_CODE_OFFSET + crate::errors::GMPError::DirectCallNotAllowed as u32;
pub const UNAUTHORIZED_ROUTER_ERROR: u32 =
    anchor_lang::error::ERROR_CODE_OFFSET + crate::errors::GMPError::UnauthorizedRouter as u32;

pub fn anchor_discriminator(instruction_name: &str) -> [u8; 8] {
    let hash = solana_sdk::hash::hash(format!("global:{instruction_name}").as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash.to_bytes()[..8]);
    disc
}

pub fn setup_program_test() -> solana_program_test::ProgramTest {
    if std::env::var("SBF_OUT_DIR").is_err() {
        let deploy_dir = std::path::Path::new(DEPLOY_DIR);
        std::env::set_var("SBF_OUT_DIR", deploy_dir);
    }

    let mut pt = solana_program_test::ProgramTest::new("ics27_gmp", crate::ID, None);
    pt.add_program("malicious_caller", MALICIOUS_CALLER_ID, None);
    pt.add_program("cpi_test_target", CPI_TEST_TARGET_ID, None);
    pt.add_program("ics26_router", ics26_router::ID, None);

    let (app_state_pda, bump) = Pubkey::find_program_address(
        &[
            crate::state::GMPAppState::SEED,
            crate::constants::GMP_PORT_ID.as_bytes(),
        ],
        &crate::ID,
    );
    let app_state = crate::state::GMPAppState {
        version: crate::state::AccountVersion::V1,
        paused: false,
        bump,
        access_manager: access_manager::ID,
        _reserved: [0; 256],
    };
    let mut data = Vec::new();
    data.extend_from_slice(crate::state::GMPAppState::DISCRIMINATOR);
    app_state.serialize(&mut data).unwrap();

    pt.add_account(
        app_state_pda,
        solana_sdk::account::Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    );

    pt
}

/// Wraps an instruction in malicious_caller::proxy_cpi.
pub fn wrap_in_proxy_cpi(payer: Pubkey, inner_ix: &Instruction) -> Instruction {
    let mut data = Vec::new();
    data.extend_from_slice(&anchor_discriminator("proxy_cpi"));

    inner_ix.data.serialize(&mut data).unwrap();

    let meta_count = inner_ix.accounts.len() as u32;
    meta_count.serialize(&mut data).unwrap();
    for meta in &inner_ix.accounts {
        meta.is_signer.serialize(&mut data).unwrap();
        meta.is_writable.serialize(&mut data).unwrap();
    }

    let mut accounts = vec![
        AccountMeta::new_readonly(inner_ix.program_id, false),
        AccountMeta::new_readonly(payer, true),
    ];
    for meta in &inner_ix.accounts {
        accounts.push(if meta.is_writable {
            AccountMeta::new(meta.pubkey, false)
        } else {
            AccountMeta::new_readonly(meta.pubkey, false)
        });
    }

    Instruction {
        program_id: MALICIOUS_CALLER_ID,
        accounts,
        data,
    }
}

/// Wraps an instruction in cpi_test_target::proxy_cpi.
pub fn wrap_in_cpi_test_target_proxy(payer: Pubkey, inner_ix: &Instruction) -> Instruction {
    let mut data = Vec::new();
    data.extend_from_slice(&anchor_discriminator("proxy_cpi"));

    inner_ix.data.serialize(&mut data).unwrap();

    let meta_count = inner_ix.accounts.len() as u32;
    meta_count.serialize(&mut data).unwrap();
    for meta in &inner_ix.accounts {
        meta.is_signer.serialize(&mut data).unwrap();
        meta.is_writable.serialize(&mut data).unwrap();
    }

    let mut accounts = vec![
        AccountMeta::new_readonly(inner_ix.program_id, false),
        AccountMeta::new_readonly(payer, true),
    ];
    for meta in &inner_ix.accounts {
        accounts.push(if meta.is_writable {
            AccountMeta::new(meta.pubkey, false)
        } else {
            AccountMeta::new_readonly(meta.pubkey, false)
        });
    }

    Instruction {
        program_id: CPI_TEST_TARGET_ID,
        accounts,
        data,
    }
}

pub async fn process_tx(
    banks_client: &solana_program_test::BanksClient,
    payer: &solana_sdk::signature::Keypair,
    recent_blockhash: solana_sdk::hash::Hash,
    instructions: &[Instruction],
) -> std::result::Result<(), solana_program_test::BanksClientError> {
    let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await
}

pub fn extract_custom_error(err: &solana_program_test::BanksClientError) -> Option<u32> {
    match err {
        solana_program_test::BanksClientError::TransactionError(
            solana_sdk::transaction::TransactionError::InstructionError(
                _,
                solana_sdk::instruction::InstructionError::Custom(code),
            ),
        ) => Some(*code),
        _ => None,
    }
}
