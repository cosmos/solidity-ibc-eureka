use crate::constants::ANCHOR_DISCRIMINATOR_SIZE;
use crate::state::*;
use anchor_lang::{AnchorSerialize, Discriminator, Space};
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

// TODO: Move to test helpers crate

pub fn create_account_data<T: Discriminator + AnchorSerialize>(account: &T) -> Vec<u8> {
    let mut data = T::DISCRIMINATOR.to_vec();
    account.serialize(&mut data).unwrap();
    data
}

pub fn setup_router_state(authority: Pubkey) -> (Pubkey, Vec<u8>) {
    let (router_state_pda, _) = Pubkey::find_program_address(&[ROUTER_STATE_SEED], &crate::ID);
    let router_state = RouterState { authority };
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
        Pubkey::find_program_address(&[CLIENT_SEED, client_id.as_bytes()], &crate::ID);

    let client = Client {
        client_id: client_id.to_string(),
        client_program_id: light_client_program,
        counterparty_info: CounterpartyInfo {
            client_id: counterparty_client_id.to_string(),
            merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
        },
        authority,
        active,
    };
    let client_data = create_account_data(&client);

    (client_pda, client_data)
}

pub fn setup_client_sequence(client_id: &str, next_sequence: u64) -> (Pubkey, Vec<u8>) {
    let (client_sequence_pda, _) =
        Pubkey::find_program_address(&[CLIENT_SEQUENCE_SEED, client_id.as_bytes()], &crate::ID);
    let client_sequence = ClientSequence {
        next_sequence_send: next_sequence,
    };
    let client_sequence_data = create_account_data(&client_sequence);
    (client_sequence_pda, client_sequence_data)
}

pub fn setup_ibc_app(port_id: &str, app_program_id: Pubkey) -> (Pubkey, Vec<u8>) {
    let (ibc_app_pda, _) =
        Pubkey::find_program_address(&[IBC_APP_SEED, port_id.as_bytes()], &crate::ID);
    let ibc_app = IBCApp {
        port_id: port_id.to_string(),
        app_program_id,
        authority: Pubkey::new_unique(),
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
            PACKET_COMMITMENT_SEED,
            source_client.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &crate::ID,
    );

    let commitment_value = crate::utils::ics24::packet_commitment_bytes32(packet);
    let commitment = Commitment {
        value: commitment_value,
        created_at: 1000, // Default timestamp for tests
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
    use anchor_lang::{AnchorDeserialize, Discriminator, Space};

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
    let mut account_data = &sequence_account.data[ANCHOR_DISCRIMINATOR_SIZE..];
    let client_sequence: ClientSequence = AnchorDeserialize::deserialize(&mut account_data)
        .expect("Failed to deserialize ClientSequence");

    client_sequence.next_sequence_send
}

pub fn get_client_sequence_from_result_by_pubkey(
    result: &mollusk_svm::result::InstructionResult,
    pubkey: &Pubkey,
) -> Option<u64> {
    use anchor_lang::{AnchorDeserialize, Discriminator};

    result
        .resulting_accounts
        .iter()
        .find(|(key, _)| key == pubkey)
        .and_then(|(_, account)| {
            // Verify it's a ClientSequence account
            if account.data.len() >= ANCHOR_DISCRIMINATOR_SIZE
                && &account.data[..ANCHOR_DISCRIMINATOR_SIZE] == ClientSequence::DISCRIMINATOR
            {
                let mut account_data = &account.data[ANCHOR_DISCRIMINATOR_SIZE..];
                let client_sequence: ClientSequence =
                    AnchorDeserialize::deserialize(&mut account_data).ok()?;
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

    let mut mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());
    mollusk.add_program(
        &MOCK_LIGHT_CLIENT_ID,
        crate::get_mock_client_program_path(),
        &solana_sdk::bpf_loader_upgradeable::ID,
    );
    mollusk.add_program(
        &MOCK_IBC_APP_PROGRAM_ID,
        crate::get_mock_ibc_app_program_path(),
        &solana_sdk::bpf_loader_upgradeable::ID,
    );
    mollusk
}

/// Setup mollusk with just the mock light client for testing scenarios that don't need IBC apps
///
/// This adds the router and mock light client programs to mollusk
pub fn setup_mollusk_with_light_client() -> mollusk_svm::Mollusk {
    use mollusk_svm::Mollusk;

    let mut mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());
    mollusk.add_program(
        &MOCK_LIGHT_CLIENT_ID,
        crate::get_mock_client_program_path(),
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
            PAYLOAD_CHUNK_SEED,
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
            PROOF_CHUNK_SEED,
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
