use crate::accounts::account_owned_by;
use crate::admin::Admin;
use crate::relayer::Relayer;
use crate::Actor;
use anchor_lang::InstructionData;
use solana_program_test::{BanksClient, BanksClientError, ProgramTest};
use solana_sdk::{
    account::Account,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
    sysvar::Sysvar as _,
    transaction::Transaction,
};

const DEPLOY_DIR: &str = "../target/deploy";
pub const TEST_CLOCK_TIME: i64 = 1_700_000_000;
const DEFAULT_PREFUND_LAMPORTS: u64 = 10_000_000_000;
const MOCK_LC_LATEST_HEIGHT: u64 = 1;

// ── Account metadata ────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct ChainAccounts {
    pub mock_client_state: Pubkey,
    pub mock_consensus_state: Pubkey,
    pub app_state_pda: Pubkey,
    pub gmp_app_state_pda: Option<Pubkey>,
    pub counter_app_state_pda: Option<Pubkey>,
    pub ift_app_state_pda: Option<Pubkey>,
}

/// Derive mock light client PDAs for any `client_id`.
pub fn derive_mock_lc_pdas(client_id: &str) -> (Pubkey, Pubkey) {
    let (client_state, _) =
        Pubkey::find_program_address(&[b"client", client_id.as_bytes()], &mock_light_client::ID);
    let (consensus_state, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_state.as_ref(),
            &MOCK_LC_LATEST_HEIGHT.to_le_bytes(),
        ],
        &mock_light_client::ID,
    );
    (client_state, consensus_state)
}

/// Programs that can be loaded onto a chain.
///
/// IBC application variants (`TestIbcApp`, `MockIbcApp`, `Ics27Gmp`) register
/// on a port and run initialization logic. Other variants only load the program
/// binary — no port registration or init.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Program {
    /// Stateful `test_ibc_app` that counts packets sent/received/acked/timed-out.
    TestIbcApp,
    /// Stateless `mock_ibc_app` with magic-string ack control
    /// (`RETURN_ERROR_ACK` / `RETURN_EMPTY_ACK`).
    MockIbcApp,
    /// `ics27_gmp` — GMP IBC application registered on the GMP port.
    Ics27Gmp,
    /// `test_gmp_app` — counter app invoked by GMP via CPI.
    TestGmpApp,
    /// `test_cpi_proxy` — generic CPI proxy for security tests.
    TestCpiProxy,
    /// `ift` — inter-chain fungible token transfers (uses GMP's port).
    Ift,
    /// `test_access_manager` — second AM instance for AM migration tests.
    TestAccessManager,
}

pub struct ChainConfig<'a> {
    pub client_id: &'a str,
    pub counterparty_client_id: &'a str,
    pub admin: &'a Admin,
    pub relayer: &'a Relayer,
    pub programs: &'a [Program],
}

// ── Chain (setup + runtime) ─────────────────────────────────────────────

pub struct Chain {
    pt: Option<ProgramTest>,
    client_id: String,
    counterparty_client_id: String,
    clock_time: i64,
    programs: Vec<Program>,
    relayer_pubkey: Pubkey,
    admin: Keypair,
    banks: Option<BanksClient>,
    payer: Option<Keypair>,
    blockhash: Hash,
    pub accounts: ChainAccounts,
    additional_counterparties: Vec<(String, String)>,
}

impl Chain {
    pub fn new(config: ChainConfig<'_>) -> Self {
        let admin = config.admin.keypair().insecure_clone();
        let accounts = derive_chain_accounts(config.client_id, config.programs);
        let pt = build_program_test(&config, &admin, &accounts);

        Self {
            pt: Some(pt),
            client_id: config.client_id.to_string(),
            counterparty_client_id: config.counterparty_client_id.to_string(),
            clock_time: TEST_CLOCK_TIME,
            programs: config.programs.to_vec(),
            relayer_pubkey: config.relayer.pubkey(),
            admin,
            banks: None,
            payer: None,
            blockhash: Hash::default(),
            accounts,
            additional_counterparties: Vec::new(),
        }
    }

    // ── Setup phase (before start) ──────────────────────────────────────

    /// Pre-fund an actor's account with the default amount (10 SOL).
    pub fn prefund(&mut self, actor: &impl Actor) {
        self.prefund_lamports(actor.pubkey(), DEFAULT_PREFUND_LAMPORTS);
    }

    /// Pre-fund an account with a specific lamport amount.
    pub fn prefund_lamports(&mut self, pubkey: Pubkey, lamports: u64) {
        self.pt().add_account(pubkey, system_account(lamports));
    }

    /// Register an additional client/counterparty pair on this chain.
    ///
    /// The mock light client and router `add_client` instructions are executed
    /// during `start()`, after the primary client is initialized.
    pub fn add_counterparty(&mut self, client_id: &str, counterparty_client_id: &str) {
        self.additional_counterparties
            .push((client_id.to_string(), counterparty_client_id.to_string()));
    }

    /// Start the chain runtime, executing all initialization transactions.
    pub async fn start(&mut self) {
        let pt = self.pt.take().expect("chain already started");
        let (banks, payer, mut blockhash) = pt.start().await;

        let steps = build_init_steps(
            payer.pubkey(),
            &self.admin,
            self.relayer_pubkey,
            &self.client_id,
            &self.counterparty_client_id,
            &self.programs,
        );

        let authority_ref = &self.admin;
        for (ixs, needs_authority) in &steps {
            let extra: &[&Keypair] = if *needs_authority {
                std::slice::from_ref(&authority_ref)
            } else {
                &[]
            };
            blockhash = send_init_tx(&banks, &payer, blockhash, ixs, extra).await;
        }

        // Initialize additional counterparties (for multi-hop tests)
        let am_pda = derive_access_manager_pda();
        let router_state_pda = derive_router_state_pda();
        for (extra_client_id, extra_counterparty_id) in &self.additional_counterparties {
            let lc_ix = build_mock_lc_initialize_ix(payer.pubkey(), extra_client_id);
            blockhash = send_init_tx(&banks, &payer, blockhash, &[lc_ix], &[]).await;

            let add_ix = build_add_client_ix(
                &self.admin,
                router_state_pda,
                am_pda,
                extra_client_id,
                extra_counterparty_id,
            );
            blockhash = send_init_tx(&banks, &payer, blockhash, &[add_ix], &[&self.admin]).await;
        }

        self.banks = Some(banks);
        self.payer = Some(payer);
        self.blockhash = blockhash;
    }

    // ── Runtime phase (after start) ─────────────────────────────────────

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn counterparty_client_id(&self) -> &str {
        &self.counterparty_client_id
    }

    pub const fn clock_time(&self) -> i64 {
        self.clock_time
    }

    pub const fn counter_app_state_pda(&self) -> Pubkey {
        self.accounts
            .counter_app_state_pda
            .expect("chain should have counter_app_state PDA")
    }

    pub const fn ift_app_state_pda(&self) -> Pubkey {
        self.accounts
            .ift_app_state_pda
            .expect("chain should have ift_app_state PDA")
    }

    pub const fn admin_keypair(&self) -> &Keypair {
        &self.admin
    }

    pub const fn payer(&self) -> &Keypair {
        self.payer.as_ref().expect("chain not started yet")
    }

    pub const fn blockhash(&self) -> Hash {
        self.blockhash
    }

    /// Submit a transaction and auto-refresh the blockhash.
    pub async fn process_transaction(&mut self, tx: Transaction) -> Result<(), BanksClientError> {
        let banks = self.banks.as_mut().expect("chain not started yet");
        banks.process_transaction(tx).await?;
        self.blockhash = banks.get_latest_blockhash().await.unwrap();
        Ok(())
    }

    /// Read an account from the chain, returning `None` if it doesn't exist.
    pub async fn get_account(&self, pubkey: Pubkey) -> Option<Account> {
        self.banks
            .as_ref()
            .expect("chain not started yet")
            .get_account(pubkey)
            .await
            .unwrap()
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    const fn pt(&mut self) -> &mut ProgramTest {
        self.pt.as_mut().expect("chain already started")
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

const fn system_account(lamports: u64) -> Account {
    Account {
        lamports,
        data: vec![],
        owner: system_program::ID,
        executable: false,
        rent_epoch: 0,
    }
}

// ── PDA derivation ──────────────────────────────────────────────────────

fn derive_access_manager_pda() -> Pubkey {
    let (pda, _) = solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    pda
}

fn derive_router_state_pda() -> Pubkey {
    let (pda, _) =
        Pubkey::find_program_address(&[ics26_router::state::RouterState::SEED], &ics26_router::ID);
    pda
}

fn derive_chain_accounts(client_id: &str, programs: &[Program]) -> ChainAccounts {
    let (mock_client_state, _) =
        Pubkey::find_program_address(&[b"client", client_id.as_bytes()], &mock_light_client::ID);
    let (mock_consensus_state, _) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            mock_client_state.as_ref(),
            &MOCK_LC_LATEST_HEIGHT.to_le_bytes(),
        ],
        &mock_light_client::ID,
    );

    let mut accounts = ChainAccounts {
        mock_client_state,
        mock_consensus_state,
        app_state_pda: Pubkey::default(),
        gmp_app_state_pda: None,
        counter_app_state_pda: None,
        ift_app_state_pda: None,
    };

    for program in programs {
        match program {
            Program::TestIbcApp => {
                let (pda, _) = Pubkey::find_program_address(
                    &[solana_ibc_types::IBCAppState::SEED],
                    &test_ibc_app::ID,
                );
                accounts.app_state_pda = pda;
            }
            Program::MockIbcApp => {
                // mock_ibc_app has no initialize — use a unique address for the dummy account
                accounts.app_state_pda = Pubkey::new_unique();
            }
            Program::Ics27Gmp => {
                let (gmp_pda, _) = Pubkey::find_program_address(
                    &[ics27_gmp::state::GMPAppState::SEED],
                    &ics27_gmp::ID,
                );
                accounts.app_state_pda = gmp_pda;
                accounts.gmp_app_state_pda = Some(gmp_pda);
            }
            Program::TestGmpApp => {
                let (counter_pda, _) = Pubkey::find_program_address(
                    &[test_gmp_app::state::CounterAppState::SEED],
                    &test_gmp_app::ID,
                );
                accounts.counter_app_state_pda = Some(counter_pda);
            }
            Program::TestCpiProxy | Program::TestAccessManager => {}
            Program::Ift => {
                let (ift_pda, _) =
                    Pubkey::find_program_address(&[ift::constants::IFT_APP_STATE_SEED], &ift::ID);
                accounts.ift_app_state_pda = Some(ift_pda);
            }
        }
    }

    accounts
}

// ── Transaction helper ──────────────────────────────────────────────────

async fn send_init_tx(
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

// ── Init step builder ────────────────────────────────────────────────────

/// Build all initialization instructions grouped by transaction.
///
/// Returns `(instructions, needs_authority_signer)` tuples that must be
/// executed sequentially.
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

fn build_init_steps(
    payer: Pubkey,
    authority: &Keypair,
    relayer_pubkey: Pubkey,
    client_id: &str,
    counterparty_client_id: &str,
    programs: &[Program],
) -> Vec<(Vec<Instruction>, bool)> {
    let am_pda = derive_access_manager_pda();
    let router_state_pda = derive_router_state_pda();
    let (port_id, app_program_id) = get_port_and_app(programs);

    let mut steps = vec![
        // TX1: access_manager::initialize
        (vec![build_am_initialize_ix(payer, authority, am_pda)], true),
        // TX2: grant RELAYER_ROLE and ID_CUSTOMIZER_ROLE
        (
            vec![
                build_am_grant_role_ix(
                    am_pda,
                    authority.pubkey(),
                    solana_ibc_types::roles::RELAYER_ROLE,
                    relayer_pubkey,
                ),
                build_am_grant_role_ix(
                    am_pda,
                    authority.pubkey(),
                    solana_ibc_types::roles::ID_CUSTOMIZER_ROLE,
                    authority.pubkey(),
                ),
            ],
            true,
        ),
        // TX3: ics26_router::initialize
        (
            vec![build_router_initialize_ix(
                payer,
                authority,
                router_state_pda,
                access_manager::ID,
            )],
            true,
        ),
        // TX4: mock_light_client::initialize
        (vec![build_mock_lc_initialize_ix(payer, client_id)], false),
        // TX5: add_client + add_ibc_app
        (
            vec![
                build_add_client_ix(
                    authority,
                    router_state_pda,
                    am_pda,
                    client_id,
                    counterparty_client_id,
                ),
                build_add_ibc_app_ix(
                    payer,
                    authority,
                    router_state_pda,
                    am_pda,
                    port_id,
                    app_program_id,
                ),
            ],
            true,
        ),
    ];

    // App-specific initialization
    for p in programs {
        match p {
            Program::TestIbcApp => {
                steps.push((
                    vec![build_test_ibc_app_initialize_ix(payer, authority.pubkey())],
                    false,
                ));
            }
            Program::Ics27Gmp => {
                steps.push((
                    vec![build_gmp_initialize_ix(
                        payer,
                        authority,
                        access_manager::ID,
                    )],
                    true,
                ));
            }
            Program::TestGmpApp => {
                steps.push((
                    vec![build_test_gmp_app_initialize_ix(payer, authority.pubkey())],
                    false,
                ));
            }
            Program::Ift => {
                steps.push((vec![build_ift_initialize_ix(payer, authority)], true));
            }
            Program::TestAccessManager => {
                let test_am_pda = derive_test_access_manager_pda();
                steps.push((
                    vec![build_test_am_initialize_ix(payer, authority, test_am_pda)],
                    true,
                ));
                steps.push((
                    vec![build_am_grant_role_ix_for_program(
                        test_am_pda,
                        authority.pubkey(),
                        solana_ibc_types::roles::ADMIN_ROLE,
                        authority.pubkey(),
                        test_access_manager::ID,
                    )],
                    true,
                ));
            }
            Program::MockIbcApp | Program::TestCpiProxy => {}
        }
    }

    steps
}

// ── Internal ProgramTest builder ────────────────────────────────────────

fn ensure_sbf_out_dir() {
    if std::env::var("SBF_OUT_DIR").is_err() {
        std::env::set_var("SBF_OUT_DIR", std::path::Path::new(DEPLOY_DIR));
    }
}

fn add_program_data(pt: &mut ProgramTest, program_id: Pubkey, authority_pubkey: Pubkey) {
    let (program_data_pda, _) =
        Pubkey::find_program_address(&[program_id.as_ref()], &bpf_loader_upgradeable::ID);
    let state = UpgradeableLoaderState::ProgramData {
        slot: 0,
        upgrade_authority_address: Some(authority_pubkey),
    };
    pt.add_account(
        program_data_pda,
        Account {
            lamports: 1_000_000,
            data: bincode::serialize(&state).unwrap(),
            owner: bpf_loader_upgradeable::ID,
            executable: false,
            rent_epoch: 0,
        },
    );
}

fn build_program_test(
    config: &ChainConfig<'_>,
    authority: &Keypair,
    accounts: &ChainAccounts,
) -> ProgramTest {
    ensure_sbf_out_dir();

    let mut pt = ProgramTest::new("ics26_router", ics26_router::ID, None);
    pt.add_program("mock_light_client", mock_light_client::ID, None);
    pt.add_program("access_manager", access_manager::ID, None);

    // ProgramData accounts for programs that verify upgrade authority
    add_program_data(&mut pt, access_manager::ID, authority.pubkey());
    add_program_data(&mut pt, ics26_router::ID, authority.pubkey());

    // App-specific programs
    for program in config.programs {
        match program {
            Program::TestIbcApp => {
                pt.add_program("test_ibc_app", test_ibc_app::ID, None);
            }
            Program::MockIbcApp => {
                pt.add_program("mock_ibc_app", mock_ibc_app::ID, None);
                // mock_ibc_app has no initialize — pre-create a dummy account
                pt.add_account(
                    accounts.app_state_pda,
                    account_owned_by(vec![0u8; 100], mock_ibc_app::ID),
                );
            }
            Program::Ics27Gmp => {
                pt.add_program("ics27_gmp", ics27_gmp::ID, None);
                add_program_data(&mut pt, ics27_gmp::ID, authority.pubkey());
            }
            Program::TestGmpApp => {
                pt.add_program("test_gmp_app", test_gmp_app::ID, None);
            }
            Program::TestCpiProxy => {
                pt.add_program("test_cpi_proxy", test_cpi_proxy::ID, None);
            }
            Program::Ift => {
                pt.add_program("ift", ift::ID, None);
                add_program_data(&mut pt, ift::ID, authority.pubkey());
            }
            Program::TestAccessManager => {
                pt.add_program("test_access_manager", test_access_manager::ID, None);
                add_program_data(&mut pt, test_access_manager::ID, authority.pubkey());
            }
        }
    }

    // Pre-fund relayer and authority
    for pubkey in [config.relayer.pubkey(), authority.pubkey()] {
        pt.add_account(pubkey, system_account(DEFAULT_PREFUND_LAMPORTS));
    }

    // Deterministic clock
    let clock = solana_sdk::clock::Clock {
        slot: 1,
        epoch_start_timestamp: TEST_CLOCK_TIME,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: TEST_CLOCK_TIME,
    };
    let mut clock_data = vec![0u8; solana_sdk::clock::Clock::size_of()];
    bincode::serialize_into(&mut clock_data[..], &clock).unwrap();
    pt.add_account(
        solana_sdk::sysvar::clock::ID,
        Account {
            lamports: 1,
            data: clock_data,
            owner: solana_sdk::sysvar::ID,
            executable: false,
            rent_epoch: 0,
        },
    );

    pt
}

// ── Instruction builders for initialization ─────────────────────────────

fn build_am_initialize_ix(payer: Pubkey, authority: &Keypair, am_pda: Pubkey) -> Instruction {
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
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: access_manager::instruction::Initialize {
            admin: authority.pubkey(),
        }
        .data(),
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

fn derive_test_access_manager_pda() -> Pubkey {
    solana_ibc_types::access_manager::AccessManager::pda(test_access_manager::ID).0
}

fn build_test_am_initialize_ix(payer: Pubkey, authority: &Keypair, am_pda: Pubkey) -> Instruction {
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
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: access_manager::instruction::Initialize {
            admin: authority.pubkey(),
        }
        .data(),
    }
}

fn build_router_initialize_ix(
    payer: Pubkey,
    authority: &Keypair,
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
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: ics26_router::instruction::Initialize {
            access_manager: access_manager_program,
        }
        .data(),
    }
}

fn build_mock_lc_initialize_ix(payer: Pubkey, chain_id: &str) -> Instruction {
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

fn build_add_client_ix(
    authority: &Keypair,
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
            AccountMeta::new(authority.pubkey(), true),
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
    authority: &Keypair,
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
            AccountMeta::new_readonly(authority.pubkey(), true),
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
    authority: &Keypair,
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
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: ics27_gmp::instruction::Initialize {
            access_manager: access_manager_program,
        }
        .data(),
    }
}

fn build_ift_initialize_ix(payer: Pubkey, authority: &Keypair) -> Instruction {
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
            AccountMeta::new_readonly(authority.pubkey(), true),
        ],
        data: ift::instruction::Initialize {
            admin: authority.pubkey(),
        }
        .data(),
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
