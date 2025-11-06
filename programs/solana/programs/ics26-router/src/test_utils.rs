use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use crate::state::*;
use anchor_lang::{AccountDeserialize, AnchorSerialize, Discriminator, Space};
use solana_ibc_types::Payload;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::sysvar::Sysvar;

pub const ANCHOR_ERROR_OFFSET: u32 = 6000;

// Mock light client program ID - must match the ID in mock-light-client/src/lib.rs
pub const MOCK_LIGHT_CLIENT_ID: Pubkey =
    solana_sdk::pubkey!("CSLS3A9jS7JAD8aUe3LRXMYZ1U8Lvxn9usGygVrA2arZ");

// Dummy IBC app program ID - must match the ID in dummy-ibc-app/src/lib.rs
pub const DUMMY_IBC_APP_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("5E73beFMq9QZvbwPN5i84psh2WcyJ9PgqF4avBaRDgCC");

// Mock IBC app program ID - must match the ID in mock-ibc-app/src/lib.rs
pub const MOCK_IBC_APP_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("9qnEj3T1NsaGkN3Sj7hgJZiKrVbKVBNmVphJ6PW1PDAB");

pub fn get_router_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("ROUTER_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/ics26_router".to_string())
    })
}

pub fn get_mock_client_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("MOCK_CLIENT_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/mock_light_client".to_string())
    })
}

pub fn get_mock_ibc_app_program_path() -> &'static str {
    use std::sync::OnceLock;
    static PATH: OnceLock<String> = OnceLock::new();

    PATH.get_or_init(|| {
        std::env::var("MOCK_IBC_APP_PROGRAM_PATH")
            .unwrap_or_else(|_| "../../target/deploy/mock_ibc_app".to_string())
    })
}

// TODO: Move to test helpers crate
pub fn create_account_data<T: Discriminator + AnchorSerialize>(account: &T) -> Vec<u8> {
    let mut data = T::DISCRIMINATOR.to_vec();
    account.serialize(&mut data).unwrap();
    data
}

pub fn setup_router_state(authority: Pubkey) -> (Pubkey, Vec<u8>) {
    let (router_state_pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);
    let router_state = RouterState {
        version: AccountVersion::V1,
        authority,
        _reserved: [0; 256],
    };
    let router_state_data = create_account_data(&router_state);
    (router_state_pda, router_state_data)
}

pub fn setup_client(
    client_id: &str,
    authority: Pubkey,
    light_client_program: Pubkey,
    counterparty_client_id: &str,
    active: bool,
) -> (Pubkey, Vec<u8>) {
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &crate::ID);

    let client = Client {
        version: AccountVersion::V1,
        client_id: client_id.to_string(),
        client_program_id: light_client_program,
        counterparty_info: CounterpartyInfo {
            client_id: counterparty_client_id.to_string(),
            merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
        },
        authority,
        active,
        _reserved: [0; 256],
    };
    let client_data = create_account_data(&client);

    (client_pda, client_data)
}

pub fn setup_client_sequence(client_id: &str, next_sequence: u64) -> (Pubkey, Vec<u8>) {
    let (client_sequence_pda, _) =
        Pubkey::find_program_address(&[ClientSequence::SEED, client_id.as_bytes()], &crate::ID);
    let client_sequence = ClientSequence {
        next_sequence_send: next_sequence,
        version: AccountVersion::V1,
        _reserved: [0; 256],
    };
    let client_sequence_data = create_account_data(&client_sequence);
    (client_sequence_pda, client_sequence_data)
}

pub fn setup_ibc_app(port_id: &str, app_program_id: Pubkey) -> (Pubkey, Vec<u8>) {
    let (ibc_app_pda, _) =
        Pubkey::find_program_address(&[IBCApp::SEED, port_id.as_bytes()], &crate::ID);
    let ibc_app = IBCApp {
        version: AccountVersion::V1,
        port_id: port_id.to_string(),
        app_program_id,
        authority: Pubkey::new_unique(),
        _reserved: [0; 256],
    };
    let ibc_app_data = create_account_data(&ibc_app);
    (ibc_app_pda, ibc_app_data)
}

pub fn create_test_packet(
    sequence: u64,
    source_client: &str,
    dest_client: &str,
    source_port: &str,
    dest_port: &str,
    timeout_timestamp: i64,
) -> Packet {
    Packet {
        sequence,
        source_client: source_client.to_string(),
        dest_client: dest_client.to_string(),
        timeout_timestamp,
        payloads: vec![Payload {
            source_port: source_port.to_string(),
            dest_port: dest_port.to_string(),
            version: "1".to_string(),
            encoding: "json".to_string(),
            value: b"test data".to_vec(),
        }],
    }
}

pub fn create_mock_light_client_accounts(
    light_client_program: &Pubkey,
) -> (Pubkey, Pubkey, Vec<(Pubkey, solana_sdk::account::Account)>) {
    let client_state = Pubkey::new_unique();
    let consensus_state = Pubkey::new_unique();

    let accounts = vec![
        (
            *light_client_program,
            solana_sdk::account::Account {
                lamports: 0,
                data: vec![],
                owner: solana_sdk::native_loader::ID,
                executable: true,
                rent_epoch: 0,
            },
        ),
        (
            client_state,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: vec![0u8; 100],
                owner: *light_client_program,
                executable: false,
                rent_epoch: 0,
            },
        ),
        (
            consensus_state,
            solana_sdk::account::Account {
                lamports: 1_000_000,
                data: vec![0u8; 100],
                owner: *light_client_program,
                executable: false,
                rent_epoch: 0,
            },
        ),
    ];

    (client_state, consensus_state, accounts)
}

pub fn create_clock_account() -> (Pubkey, solana_sdk::account::Account) {
    (
        solana_sdk::sysvar::clock::ID,
        solana_sdk::account::Account {
            lamports: 1,
            data: vec![1u8; solana_sdk::clock::Clock::size_of()],
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_clock_account_with_data(
    clock_data: Vec<u8>,
) -> (Pubkey, solana_sdk::account::Account) {
    (
        solana_sdk::sysvar::clock::ID,
        solana_sdk::account::Account {
            lamports: 1,
            data: clock_data,
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_instructions_sysvar_account() -> (Pubkey, solana_sdk::account::Account) {
    use solana_sdk::sysvar::instructions::{
        construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
    };

    // Create minimal mock instructions to simulate router calling IBC app via CPI
    // For CPI validation, only the program_id matters - GMP apps check that
    // the calling instruction's program_id matches their authorized router
    //
    // Instruction 0: The router instruction (current when router executes)
    // This simulates the top-level recv_packet/ack_packet/timeout_packet call
    let account_pubkey = Pubkey::new_unique();
    let account = BorrowedAccountMeta {
        pubkey: &account_pubkey,
        is_signer: false,
        is_writable: true,
    };
    let mock_router_ix = BorrowedInstruction {
        program_id: &crate::ID,
        accounts: vec![account],
        data: &[],
    };

    // Serialize instructions for sysvar
    // When GMP apps check the sysvar during CPI, they'll see the router as the caller
    let ixs_data = construct_instructions_data(&[mock_router_ix]);

    (
        solana_sdk::sysvar::instructions::ID,
        solana_sdk::account::Account {
            lamports: 1_000_000,
            data: ixs_data,
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_account(
    pubkey: Pubkey,
    data: Vec<u8>,
    owner: Pubkey,
) -> (Pubkey, solana_sdk::account::Account) {
    use solana_sdk::rent::Rent;

    let rent = Rent::default().minimum_balance(data.len());

    (
        pubkey,
        solana_sdk::account::Account {
            lamports: rent.max(1_000_000), // Use at least 1M lamports or rent-exempt amount
            data,
            owner,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_system_account(pubkey: Pubkey) -> (Pubkey, solana_sdk::account::Account) {
    (
        pubkey,
        solana_sdk::account::Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_program_account(pubkey: Pubkey) -> (Pubkey, solana_sdk::account::Account) {
    (
        pubkey,
        solana_sdk::account::Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        },
    )
}

pub fn create_uninitialized_commitment_account(
    pubkey: Pubkey,
) -> (Pubkey, solana_sdk::account::Account) {
    use solana_sdk::rent::Rent;

    let account_size = 8 + Commitment::INIT_SPACE;

    (
        pubkey,
        solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(account_size),
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn create_uninitialized_account(
    pubkey: Pubkey,
    lamports: u64,
) -> (Pubkey, solana_sdk::account::Account) {
    (
        pubkey,
        solana_sdk::account::Account {
            lamports,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

pub fn setup_packet_commitment(
    source_client: &str,
    sequence: u64,
    packet: &Packet,
) -> (Pubkey, Vec<u8>) {
    let (packet_commitment_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            source_client.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &crate::ID,
    );

    let commitment_value = crate::utils::ics24::packet_commitment_bytes32(packet);
    let commitment = Commitment {
        value: commitment_value,
    };
    let commitment_data = create_account_data(&commitment);

    (packet_commitment_pda, commitment_data)
}

pub const DISCRIMINATOR_SIZE: usize = 8;

pub fn get_account_data_from_mollusk_result(
    result: &mollusk_svm::result::InstructionResult,
    index: usize,
) -> &[u8] {
    let (_, account) = &result.resulting_accounts[index];
    &account.data[DISCRIMINATOR_SIZE..]
}

pub fn get_account_data_from_mollusk<'a>(
    result: &'a mollusk_svm::result::InstructionResult,
    pubkey: &Pubkey,
) -> Option<&'a [u8]> {
    result
        .resulting_accounts
        .iter()
        .find(|(key, _)| key == pubkey)
        .map(|(_, account)| &account.data[DISCRIMINATOR_SIZE..])
}

pub fn get_client_sequence_from_result(result: &mollusk_svm::result::InstructionResult) -> u64 {
    use anchor_lang::{Discriminator, Space};

    // ClientSequence discriminator to verify account type
    let expected_discriminator = ClientSequence::DISCRIMINATOR;
    let account_size = 8 + ClientSequence::INIT_SPACE;

    // Find the client_sequence account by checking discriminator, size and owner
    let (_, sequence_account) = result
        .resulting_accounts
        .iter()
        .find(|(_, account)| {
            account.data.len() == account_size
                && account.owner == crate::ID
                && account.data.len() >= ANCHOR_DISCRIMINATOR_SIZE
                && &account.data[..ANCHOR_DISCRIMINATOR_SIZE] == expected_discriminator
        })
        .expect("client_sequence account not found");

    // Deserialize the account properly
    let client_sequence: ClientSequence =
        ClientSequence::try_deserialize(&mut &sequence_account.data[..])
            .expect("Failed to deserialize ClientSequence");

    client_sequence.next_sequence_send
}

pub fn get_client_sequence_from_result_by_pubkey(
    result: &mollusk_svm::result::InstructionResult,
    pubkey: &Pubkey,
) -> Option<u64> {
    use anchor_lang::Discriminator;

    result
        .resulting_accounts
        .iter()
        .find(|(key, _)| key == pubkey)
        .and_then(|(_, account)| {
            // Verify it's a ClientSequence account
            if account.data.len() >= ANCHOR_DISCRIMINATOR_SIZE
                && &account.data[..ANCHOR_DISCRIMINATOR_SIZE] == ClientSequence::DISCRIMINATOR
            {
                let client_sequence: ClientSequence =
                    ClientSequence::try_deserialize(&mut &account.data[..]).ok()?;
                Some(client_sequence.next_sequence_send)
            } else {
                None
            }
        })
}

/// Setup mollusk with mock programs for testing
///
/// This adds the router, mock light client, and mock IBC app programs to mollusk
pub fn setup_mollusk_with_mock_programs() -> mollusk_svm::Mollusk {
    use mollusk_svm::Mollusk;

    let mut mollusk = Mollusk::new(&crate::ID, get_router_program_path());
    mollusk.add_program(
        &MOCK_LIGHT_CLIENT_ID,
        get_mock_client_program_path(),
        &solana_sdk::bpf_loader_upgradeable::ID,
    );
    mollusk.add_program(
        &MOCK_IBC_APP_PROGRAM_ID,
        get_mock_ibc_app_program_path(),
        &solana_sdk::bpf_loader_upgradeable::ID,
    );
    mollusk
}

/// Setup mollusk with just the mock light client for testing scenarios that don't need IBC apps
///
/// This adds the router and mock light client programs to mollusk
pub fn setup_mollusk_with_light_client() -> mollusk_svm::Mollusk {
    use mollusk_svm::Mollusk;

    let mut mollusk = Mollusk::new(&crate::ID, get_router_program_path());
    mollusk.add_program(
        &MOCK_LIGHT_CLIENT_ID,
        get_mock_client_program_path(),
        &solana_sdk::bpf_loader_upgradeable::ID,
    );
    mollusk
}

fn assert_dummy_app_counter(
    result: &mollusk_svm::result::InstructionResult,
    dummy_app_state_pubkey: &Pubkey,
    offset: usize,
    expected_count: u64,
    counter_name: &str,
) {
    let dummy_app_state_data = get_account_data_from_mollusk(result, dummy_app_state_pubkey)
        .expect("dummy app state account not found");

    let actual_count = u64::from_le_bytes(
        dummy_app_state_data[offset..offset + std::mem::size_of::<u64>()]
            .try_into()
            .unwrap(),
    );

    assert_eq!(
        actual_count, expected_count,
        "dummy IBC app {counter_name} counter should be {expected_count} after CPI call"
    );
}

pub fn create_bpf_program_account(pubkey: Pubkey) -> (Pubkey, solana_sdk::account::Account) {
    (
        pubkey,
        solana_sdk::account::Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::bpf_loader_upgradeable::ID,
            executable: true,
            rent_epoch: 0,
        },
    )
}

/// Helper function to create a payload chunk account for tests
/// Note: Creates accounts with 0 lamports since they will be cleaned up during test execution
pub fn create_payload_chunk_account(
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    payload_index: u8,
    chunk_index: u8,
    chunk_data: Vec<u8>,
) -> (Pubkey, solana_sdk::account::Account) {
    let (chunk_pda, _) = Pubkey::find_program_address(
        &[
            PayloadChunk::SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[payload_index],
            &[chunk_index],
        ],
        &crate::ID,
    );

    let payload_chunk = PayloadChunk {
        client_id: client_id.to_string(),
        sequence,
        payload_index,
        chunk_index,
        chunk_data,
    };

    let chunk_account_data = create_account_data(&payload_chunk);

    // Create account with 0 lamports to avoid UnbalancedInstruction errors when cleaned up
    (
        chunk_pda,
        solana_sdk::account::Account {
            lamports: 0,
            data: chunk_account_data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}

/// Helper function to create a proof chunk account for tests
/// Note: Creates accounts with 0 lamports since they will be cleaned up during test execution
pub fn create_proof_chunk_account(
    submitter: Pubkey,
    client_id: &str,
    sequence: u64,
    chunk_index: u8,
    chunk_data: Vec<u8>,
) -> (Pubkey, solana_sdk::account::Account) {
    let (chunk_pda, _) = Pubkey::find_program_address(
        &[
            ProofChunk::SEED,
            submitter.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[chunk_index],
        ],
        &crate::ID,
    );

    let proof_chunk = ProofChunk {
        client_id: client_id.to_string(),
        sequence,
        chunk_index,
        chunk_data,
    };

    let chunk_account_data = create_account_data(&proof_chunk);

    // Create account with 0 lamports to avoid UnbalancedInstruction errors when cleaned up
    (
        chunk_pda,
        solana_sdk::account::Account {
            lamports: 0,
            data: chunk_account_data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        },
    )
}
/// Assert that an instruction failed with a specific error code
pub fn assert_error_code(
    result: mollusk_svm::result::InstructionResult,
    expected_error: crate::errors::RouterError,
    test_name: &str,
) {
    match result.program_result {
        mollusk_svm::result::ProgramResult::Success => {
            panic!("Expected {test_name} to fail with {expected_error:?}, but it succeeded");
        }
        mollusk_svm::result::ProgramResult::Failure(error) => {
            if let Some(code) = get_error_code(&error) {
                let expected_code = expected_error as u32 + ANCHOR_ERROR_OFFSET;
                assert_eq!(
                    code, expected_code,
                    "Expected {expected_error:?} ({expected_code}), but got error code {code}"
                );
            } else {
                panic!("Expected custom error code for {test_name}, got: {error:?}");
            }
        }
        mollusk_svm::result::ProgramResult::UnknownError(error) => {
            panic!("Expected custom error for {test_name}, got unknown error: {error:?}");
        }
    }
}

fn get_error_code(error: &anchor_lang::prelude::ProgramError) -> Option<u32> {
    match error {
        anchor_lang::prelude::ProgramError::Custom(code) => Some(*code),
        _ => None,
    }
}
