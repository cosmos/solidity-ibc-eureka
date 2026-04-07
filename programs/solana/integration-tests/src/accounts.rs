use access_manager::AccessManagerState;
use anchor_lang::{AccountSerialize, AnchorSerialize, Discriminator, Space};
use ics26_router::state::*;
use solana_sdk::{pubkey::Pubkey, rent::Rent};

pub fn create_account_data<T: Discriminator + AnchorSerialize>(account: &T) -> Vec<u8> {
    let mut data = T::DISCRIMINATOR.to_vec();
    account.serialize(&mut data).unwrap();
    data
}

pub fn anchor_discriminator(instruction_name: &str) -> [u8; 8] {
    let hash = solana_sdk::hash::hash(format!("global:{instruction_name}").as_bytes());
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&hash.to_bytes()[..8]);
    disc
}

pub fn account_owned_by(data: Vec<u8>, owner: Pubkey) -> solana_sdk::account::Account {
    let rent = Rent::default();
    solana_sdk::account::Account {
        lamports: rent.minimum_balance(data.len()),
        data,
        owner,
        executable: false,
        rent_epoch: 0,
    }
}

pub fn setup_router_state() -> (Pubkey, Vec<u8>) {
    let (pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let state = RouterState {
        version: AccountVersion::V1,
        am_state: AccessManagerState::new(access_manager::ID),
        paused: false,
        _reserved: [0; 256],
    };
    let mut data = vec![0u8; 8 + RouterState::INIT_SPACE];
    state.try_serialize(&mut &mut data[..]).unwrap();
    (pda, data)
}

pub fn setup_client(
    client_id: &str,
    light_client_program: Pubkey,
    counterparty_client_id: &str,
    active: bool,
) -> (Pubkey, Vec<u8>) {
    let (pda, _) =
        Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &ics26_router::ID);
    let client = Client {
        version: AccountVersion::V1,
        client_id: client_id.to_string(),
        client_program_id: light_client_program,
        counterparty_info: CounterpartyInfo {
            client_id: counterparty_client_id.to_string(),
            merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
        },
        active,
        _reserved: [0; 256],
    };
    (pda, create_account_data(&client))
}

pub fn setup_ibc_app(port_id: &str, app_program_id: Pubkey) -> (Pubkey, Vec<u8>) {
    let (pda, _) =
        Pubkey::find_program_address(&[IBCApp::SEED, port_id.as_bytes()], &ics26_router::ID);
    let app = IBCApp {
        version: AccountVersion::V1,
        port_id: port_id.to_string(),
        app_program_id,
        _reserved: [0; 256],
    };
    (pda, create_account_data(&app))
}

pub fn setup_access_manager_with_roles(roles: &[(u64, &[Pubkey])]) -> (Pubkey, Vec<u8>) {
    let (pda, _) = solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);

    let mut role_data: Vec<access_manager::RoleData> = roles
        .iter()
        .map(|(role_id, members)| access_manager::RoleData {
            role_id: *role_id,
            members: members.to_vec(),
        })
        .collect();

    if !role_data
        .iter()
        .any(|r| r.role_id == solana_ibc_types::roles::ADMIN_ROLE)
    {
        role_data.push(access_manager::RoleData {
            role_id: solana_ibc_types::roles::ADMIN_ROLE,
            members: vec![Pubkey::new_unique()],
        });
    }

    let am = access_manager::state::AccessManager {
        roles: role_data,
        whitelisted_programs: vec![],
        pending_authority_transfers: vec![],
    };
    let mut data = access_manager::state::AccessManager::DISCRIMINATOR.to_vec();
    am.serialize(&mut data).unwrap();
    (pda, data)
}

pub fn setup_packet_commitment(
    source_client: &str,
    sequence: u64,
    packet: &Packet,
) -> (Pubkey, Vec<u8>) {
    let (pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            source_client.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );
    let value = solana_ibc_types::ics24::packet_commitment_bytes32(packet);
    let commitment = Commitment { value };
    (pda, create_account_data(&commitment))
}

pub fn setup_gmp_app_state(bump: u8, paused: bool) -> (Pubkey, Vec<u8>) {
    let (pda, _) =
        Pubkey::find_program_address(&[ics27_gmp::state::GMPAppState::SEED], &ics27_gmp::ID);
    let state = ics27_gmp::state::GMPAppState {
        version: ics27_gmp::state::AccountVersion::V1,
        paused,
        bump,
        am_state: AccessManagerState::new(access_manager::ID),
        _reserved: [0; 256],
    };
    let mut data = vec![0u8; 8 + ics27_gmp::state::GMPAppState::INIT_SPACE];
    state.try_serialize(&mut &mut data[..]).unwrap();
    (pda, data)
}

pub fn setup_counter_app_state(bump: u8, authority: Pubkey) -> (Pubkey, Vec<u8>) {
    let (pda, _) = Pubkey::find_program_address(
        &[test_gmp_app::state::CounterAppState::SEED],
        &test_gmp_app::ID,
    );
    let state = test_gmp_app::state::CounterAppState {
        authority,
        total_counters: 0,
        total_gmp_calls: 0,
        bump,
    };
    (pda, create_account_data(&state))
}
