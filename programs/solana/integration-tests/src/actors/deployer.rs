use super::Actor;
use crate::chain::{Program, MOCK_LC_LATEST_HEIGHT};
use anchor_lang::InstructionData;
use solana_program_test::BanksClient;
use solana_sdk::{
    bpf_loader_upgradeable,
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
    transaction::Transaction,
};

/// Which extra signers a particular init step requires.
#[derive(Clone, Copy)]
pub(crate) enum ExtraSigners {
    None,
    Deployer,
    Admin,
}

/// Deployer for the chain — holds the upgrade authority keypair and
/// orchestrates all program initialization during `Chain::start()`.
///
/// After deployment, upgrade authority for all programs is transferred
/// to the access manager PDA so governance controls upgrades.
pub struct Deployer {
    keypair: Keypair,
}

impl Default for Deployer {
    fn default() -> Self {
        Self::new()
    }
}

impl Actor for Deployer {
    fn pubkey(&self) -> Pubkey {
        self.keypair.pubkey()
    }
}

impl Deployer {
    pub fn new() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    pub const fn from_keypair(keypair: Keypair) -> Self {
        Self { keypair }
    }

    pub(crate) fn insecure_clone(&self) -> Self {
        Self::from_keypair(self.keypair.insecure_clone())
    }

    pub const fn keypair(&self) -> &Keypair {
        &self.keypair
    }
}

// ── PDA derivation ──────────────────────────────────────────────────────

pub(crate) fn derive_access_manager_pda() -> Pubkey {
    solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID).0
}

pub(crate) fn derive_router_state_pda() -> Pubkey {
    Pubkey::find_program_address(&[ics26_router::state::RouterState::SEED], &ics26_router::ID).0
}

fn derive_test_access_manager_pda() -> Pubkey {
    solana_ibc_types::access_manager::AccessManager::pda(test_access_manager::ID).0
}

// ── Transaction helper ──────────────────────────────────────────────────

pub(crate) async fn send_init_tx(
    banks: &BanksClient,
    payer: &Keypair,
    blockhash: Hash,
    ixs: &[Instruction],
    extra_signers: &[&Keypair],
) -> Hash {
    let mut signers: Vec<&Keypair> = vec![payer];
    signers.extend(extra_signers);
    let tx = Transaction::new_signed_with_payer(ixs, Some(&payer.pubkey()), &signers, blockhash);
    banks.process_transaction(tx).await.expect("init tx failed");
    banks.get_latest_blockhash().await.unwrap()
}

// ── Init step builder ───────────────────────────────────────────────────

/// Return the (`port_id`, `program_id`) for the first IBC application in the list.
fn get_port_and_app(programs: &[Program]) -> (&str, Pubkey) {
    for p in programs {
        match p {
            Program::TestIbcApp => return (crate::router::PORT_ID, test_ibc_app::ID),
            Program::MockIbcApp => return (crate::router::PORT_ID, mock_ibc_app::ID),
            Program::Ics27Gmp => return (crate::gmp::GMP_PORT_ID, ics27_gmp::ID),
            Program::TestGmpApp
            | Program::TestCpiProxy
            | Program::Ift
            | Program::TestAccessManager => {}
        }
    }
    panic!("no IBC application in programs list");
}

/// Build all initialization instructions grouped by transaction.
///
/// Returns `(instructions, extra_signers)` tuples that must be executed
/// sequentially. `admin_pubkey` is installed as the AM admin and IFT
/// admin; the deployer only signs for upgrade-authority-gated instructions.
pub(crate) fn build_init_steps(
    payer: Pubkey,
    deployer: &Keypair,
    admin_pubkey: Pubkey,
    relayer_pubkey: Pubkey,
    client_id: &str,
    counterparty_client_id: &str,
    programs: &[Program],
) -> Vec<(Vec<Instruction>, ExtraSigners)> {
    let am_pda = derive_access_manager_pda();
    let router_state_pda = derive_router_state_pda();
    let (port_id, app_program_id) = get_port_and_app(programs);

    let mut steps = vec![
        // TX1: access_manager::initialize (deployer = upgrade authority)
        (
            vec![build_am_initialize_ix(
                payer,
                deployer,
                am_pda,
                admin_pubkey,
            )],
            ExtraSigners::Deployer,
        ),
        // TX2: grant RELAYER_ROLE and ID_CUSTOMIZER_ROLE (admin = AM admin)
        (
            vec![
                build_am_grant_role_ix(
                    am_pda,
                    admin_pubkey,
                    solana_ibc_types::roles::RELAYER_ROLE,
                    relayer_pubkey,
                ),
                build_am_grant_role_ix(
                    am_pda,
                    admin_pubkey,
                    solana_ibc_types::roles::ID_CUSTOMIZER_ROLE,
                    admin_pubkey,
                ),
            ],
            ExtraSigners::Admin,
        ),
        // TX3: ics26_router::initialize (deployer = upgrade authority)
        (
            vec![build_router_initialize_ix(
                payer,
                deployer,
                router_state_pda,
                access_manager::ID,
            )],
            ExtraSigners::Deployer,
        ),
        // TX4: mock_light_client::initialize
        (
            vec![build_mock_lc_initialize_ix(payer, client_id)],
            ExtraSigners::None,
        ),
        // TX5: add_client + add_ibc_app (admin = ID_CUSTOMIZER_ROLE holder)
        (
            vec![
                build_add_client_ix(
                    admin_pubkey,
                    router_state_pda,
                    am_pda,
                    client_id,
                    counterparty_client_id,
                ),
                build_add_ibc_app_ix(
                    payer,
                    admin_pubkey,
                    router_state_pda,
                    am_pda,
                    port_id,
                    app_program_id,
                ),
            ],
            ExtraSigners::Admin,
        ),
    ];

    // App-specific initialization
    for p in programs {
        match p {
            Program::TestIbcApp => {
                steps.push((
                    vec![build_test_ibc_app_initialize_ix(payer, deployer.pubkey())],
                    ExtraSigners::None,
                ));
            }
            Program::Ics27Gmp => {
                steps.push((
                    vec![build_gmp_initialize_ix(payer, deployer, access_manager::ID)],
                    ExtraSigners::Deployer,
                ));
            }
            Program::TestGmpApp => {
                steps.push((
                    vec![build_test_gmp_app_initialize_ix(payer, deployer.pubkey())],
                    ExtraSigners::None,
                ));
            }
            Program::Ift => {
                steps.push((
                    vec![build_ift_initialize_ix(payer, deployer, admin_pubkey)],
                    ExtraSigners::Deployer,
                ));
            }
            Program::TestAccessManager => {
                let test_am_pda = derive_test_access_manager_pda();
                steps.push((
                    vec![build_test_am_initialize_ix(
                        payer,
                        deployer,
                        test_am_pda,
                        admin_pubkey,
                    )],
                    ExtraSigners::Deployer,
                ));
                steps.push((
                    vec![build_am_grant_role_ix_for_program(
                        test_am_pda,
                        admin_pubkey,
                        solana_ibc_types::roles::ADMIN_ROLE,
                        admin_pubkey,
                        test_access_manager::ID,
                    )],
                    ExtraSigners::Admin,
                ));
            }
            Program::MockIbcApp | Program::TestCpiProxy => {}
        }
    }

    // Final step: transfer upgrade authority to access manager PDA
    let authority_transfer_ixs =
        build_transfer_upgrade_authority_ixs(deployer.pubkey(), am_pda, programs);
    if !authority_transfer_ixs.is_empty() {
        steps.push((authority_transfer_ixs, ExtraSigners::Deployer));
    }

    steps
}

// ── Upgrade authority transfer ──────────────────────────────────────────

/// Build instructions to transfer upgrade authority of all deployed programs
/// to the access manager PDA, reflecting a production deployment where
/// governance controls upgrades.
fn build_transfer_upgrade_authority_ixs(
    deployer: Pubkey,
    am_pda: Pubkey,
    programs: &[Program],
) -> Vec<Instruction> {
    let mut ixs = vec![
        bpf_loader_upgradeable::set_upgrade_authority(
            &access_manager::ID,
            &deployer,
            Some(&am_pda),
        ),
        bpf_loader_upgradeable::set_upgrade_authority(&ics26_router::ID, &deployer, Some(&am_pda)),
    ];

    for p in programs {
        match p {
            Program::Ics27Gmp => {
                ixs.push(bpf_loader_upgradeable::set_upgrade_authority(
                    &ics27_gmp::ID,
                    &deployer,
                    Some(&am_pda),
                ));
            }
            Program::Ift => {
                ixs.push(bpf_loader_upgradeable::set_upgrade_authority(
                    &ift::ID,
                    &deployer,
                    Some(&am_pda),
                ));
            }
            Program::TestAccessManager => {
                ixs.push(bpf_loader_upgradeable::set_upgrade_authority(
                    &test_access_manager::ID,
                    &deployer,
                    Some(&am_pda),
                ));
            }
            _ => {}
        }
    }

    ixs
}

// ── Instruction builders for initialization ─────────────────────────────

fn build_am_initialize_ix(
    payer: Pubkey,
    deployer: &Keypair,
    am_pda: Pubkey,
    admin: Pubkey,
) -> Instruction {
    let (program_data_pda, _) =
        Pubkey::find_program_address(&[access_manager::ID.as_ref()], &bpf_loader_upgradeable::ID);

    Instruction {
        program_id: access_manager::ID,
        accounts: vec![
            AccountMeta::new(am_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(program_data_pda, false),
            AccountMeta::new_readonly(deployer.pubkey(), true),
        ],
        data: access_manager::instruction::Initialize { admin }.data(),
    }
}

fn build_am_grant_role_ix(
    am_pda: Pubkey,
    admin: Pubkey,
    role_id: u64,
    account: Pubkey,
) -> Instruction {
    build_am_grant_role_ix_for_program(am_pda, admin, role_id, account, access_manager::ID)
}

fn build_am_grant_role_ix_for_program(
    am_pda: Pubkey,
    admin: Pubkey,
    role_id: u64,
    account: Pubkey,
    am_program_id: Pubkey,
) -> Instruction {
    // Instruction discriminator is identical for access_manager and test_access_manager
    // (symlinked source), so we can reuse access_manager::instruction::GrantRole.
    Instruction {
        program_id: am_program_id,
        accounts: vec![
            AccountMeta::new(am_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: access_manager::instruction::GrantRole { role_id, account }.data(),
    }
}

fn build_test_am_initialize_ix(
    payer: Pubkey,
    deployer: &Keypair,
    am_pda: Pubkey,
    admin: Pubkey,
) -> Instruction {
    let (program_data_pda, _) = Pubkey::find_program_address(
        &[test_access_manager::ID.as_ref()],
        &bpf_loader_upgradeable::ID,
    );

    Instruction {
        program_id: test_access_manager::ID,
        accounts: vec![
            AccountMeta::new(am_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(program_data_pda, false),
            AccountMeta::new_readonly(deployer.pubkey(), true),
        ],
        data: access_manager::instruction::Initialize { admin }.data(),
    }
}

fn build_router_initialize_ix(
    payer: Pubkey,
    deployer: &Keypair,
    router_state_pda: Pubkey,
    access_manager_program: Pubkey,
) -> Instruction {
    let (program_data_pda, _) =
        Pubkey::find_program_address(&[ics26_router::ID.as_ref()], &bpf_loader_upgradeable::ID);

    Instruction {
        program_id: ics26_router::ID,
        accounts: vec![
            AccountMeta::new(router_state_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(program_data_pda, false),
            AccountMeta::new_readonly(deployer.pubkey(), true),
        ],
        data: ics26_router::instruction::Initialize {
            access_manager: access_manager_program,
        }
        .data(),
    }
}

pub(crate) fn build_mock_lc_initialize_ix(payer: Pubkey, chain_id: &str) -> Instruction {
    let (client_state_pda, _) =
        Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], &mock_light_client::ID);
    let (consensus_state_pda, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_state_pda.as_ref(),
            &MOCK_LC_LATEST_HEIGHT.to_le_bytes(),
        ],
        &mock_light_client::ID,
    );

    Instruction {
        program_id: mock_light_client::ID,
        accounts: vec![
            AccountMeta::new(client_state_pda, false),
            AccountMeta::new(consensus_state_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: mock_light_client::instruction::Initialize {
            _chain_id: chain_id.to_string(),
            _latest_height: MOCK_LC_LATEST_HEIGHT,
            _client_state: vec![],
            _consensus_state: vec![],
        }
        .data(),
    }
}

pub(crate) fn build_add_client_ix(
    admin: Pubkey,
    router_state_pda: Pubkey,
    am_pda: Pubkey,
    client_id: &str,
    counterparty_client_id: &str,
) -> Instruction {
    let (client_pda, _) = Pubkey::find_program_address(
        &[ics26_router::state::Client::SEED, client_id.as_bytes()],
        &ics26_router::ID,
    );

    Instruction {
        program_id: ics26_router::ID,
        accounts: vec![
            AccountMeta::new(admin, true),
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new_readonly(am_pda, false),
            AccountMeta::new(client_pda, false),
            AccountMeta::new_readonly(mock_light_client::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ics26_router::instruction::AddClient {
            client_id: client_id.to_string(),
            counterparty_info: ics26_router::state::CounterpartyInfo {
                client_id: counterparty_client_id.to_string(),
                merkle_prefix: vec![vec![0x01, 0x02, 0x03]],
            },
        }
        .data(),
    }
}

fn build_add_ibc_app_ix(
    payer: Pubkey,
    admin: Pubkey,
    router_state_pda: Pubkey,
    am_pda: Pubkey,
    port_id: &str,
    app_program_id: Pubkey,
) -> Instruction {
    let (ibc_app_pda, _) = Pubkey::find_program_address(
        &[ics26_router::state::IBCApp::SEED, port_id.as_bytes()],
        &ics26_router::ID,
    );

    Instruction {
        program_id: ics26_router::ID,
        accounts: vec![
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new_readonly(am_pda, false),
            AccountMeta::new(ibc_app_pda, false),
            AccountMeta::new_readonly(app_program_id, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ics26_router::instruction::AddIbcApp {
            port_id: port_id.to_string(),
        }
        .data(),
    }
}

fn build_test_ibc_app_initialize_ix(payer: Pubkey, authority: Pubkey) -> Instruction {
    let (app_state_pda, _) =
        Pubkey::find_program_address(&[solana_ibc_types::IBCAppState::SEED], &test_ibc_app::ID);

    Instruction {
        program_id: test_ibc_app::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: test_ibc_app::instruction::Initialize { authority }.data(),
    }
}

fn build_gmp_initialize_ix(
    payer: Pubkey,
    deployer: &Keypair,
    access_manager_program: Pubkey,
) -> Instruction {
    let (app_state_pda, _) =
        Pubkey::find_program_address(&[ics27_gmp::state::GMPAppState::SEED], &ics27_gmp::ID);
    let (program_data_pda, _) =
        Pubkey::find_program_address(&[ics27_gmp::ID.as_ref()], &bpf_loader_upgradeable::ID);

    Instruction {
        program_id: ics27_gmp::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(program_data_pda, false),
            AccountMeta::new_readonly(deployer.pubkey(), true),
        ],
        data: ics27_gmp::instruction::Initialize {
            access_manager: access_manager_program,
        }
        .data(),
    }
}

fn build_ift_initialize_ix(payer: Pubkey, deployer: &Keypair, admin: Pubkey) -> Instruction {
    let (app_state_pda, _) =
        Pubkey::find_program_address(&[ift::constants::IFT_APP_STATE_SEED], &ift::ID);
    let (program_data_pda, _) =
        Pubkey::find_program_address(&[ift::ID.as_ref()], &bpf_loader_upgradeable::ID);

    Instruction {
        program_id: ift::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(program_data_pda, false),
            AccountMeta::new_readonly(deployer.pubkey(), true),
        ],
        data: ift::instruction::Initialize { admin }.data(),
    }
}

fn build_test_gmp_app_initialize_ix(payer: Pubkey, authority: Pubkey) -> Instruction {
    let (app_state_pda, _) = Pubkey::find_program_address(
        &[test_gmp_app::state::CounterAppState::SEED],
        &test_gmp_app::ID,
    );

    Instruction {
        program_id: test_gmp_app::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: test_gmp_app::instruction::Initialize { authority }.data(),
    }
}
