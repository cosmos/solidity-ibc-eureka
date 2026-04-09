use crate::accounts::account_owned_by;
use crate::actors::deployer::Deployer;
use crate::Actor;
use solana_program_test::{BanksClient, BanksClientError, ProgramTest};
use solana_sdk::{
    account::Account,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    hash::Hash,
    pubkey::Pubkey,
    signature::Keypair,
    system_program,
    sysvar::Sysvar as _,
    transaction::Transaction,
};

const DEPLOY_DIR: &str = "../target/deploy";
pub const TEST_CLOCK_TIME: i64 = 1_700_000_000;
const DEFAULT_PREFUND_LAMPORTS: u64 = 10_000_000_000;
pub(crate) const MOCK_LC_LATEST_HEIGHT: u64 = 1;

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
    pub deployer: &'a Deployer,
    pub programs: &'a [Program],
}

// ── Chain (setup + runtime) ─────────────────────────────────────────────

pub struct Chain {
    pt: Option<ProgramTest>,
    client_id: String,
    counterparty_client_id: String,
    clock_time: i64,
    programs: Vec<Program>,
    banks: Option<BanksClient>,
    payer: Option<Keypair>,
    blockhash: Hash,
    pub accounts: ChainAccounts,
}

impl Chain {
    pub fn new(config: ChainConfig<'_>) -> Self {
        let accounts = derive_chain_accounts(config.client_id, config.programs);
        let pt = build_program_test(&config, &accounts);

        Self {
            pt: Some(pt),
            client_id: config.client_id.to_string(),
            counterparty_client_id: config.counterparty_client_id.to_string(),
            clock_time: TEST_CLOCK_TIME,
            programs: config.programs.to_vec(),
            banks: None,
            payer: None,
            blockhash: Hash::default(),
            accounts,
        }
    }

    // ── Setup phase (before start) ──────────────────────────────────────

    /// Pre-fund actor accounts with the default amount (10 SOL each).
    pub fn prefund(&mut self, actors: &[&dyn Actor]) {
        for actor in actors {
            self.prefund_lamports(actor.pubkey(), DEFAULT_PREFUND_LAMPORTS);
        }
    }

    /// Pre-fund an account with a specific lamport amount.
    pub fn prefund_lamports(&mut self, pubkey: Pubkey, lamports: u64) {
        self.pt().add_account(pubkey, system_account(lamports));
    }

    /// Start the `ProgramTest` runtime, producing a `BanksClient`.
    ///
    /// After calling `start()`, use `Deployer::init_programs` and
    /// `Deployer::transfer_upgrade_authority` to initialize on-chain state.
    pub async fn start(&mut self) {
        let pt = self.pt.take().expect("chain already started");
        let (banks, payer, blockhash) = pt.start().await;
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

    pub fn programs(&self) -> &[Program] {
        &self.programs
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

// ── Internal ProgramTest builder ────────────────────────────────────────

fn ensure_sbf_out_dir() {
    if std::env::var("SBF_OUT_DIR").is_err() {
        std::env::set_var("SBF_OUT_DIR", std::path::Path::new(DEPLOY_DIR));
    }
}

fn add_program_data(pt: &mut ProgramTest, program_id: Pubkey, deployer_pubkey: Pubkey) {
    let (program_data_pda, _) =
        Pubkey::find_program_address(&[program_id.as_ref()], &bpf_loader_upgradeable::ID);
    let state = UpgradeableLoaderState::ProgramData {
        slot: 0,
        upgrade_authority_address: Some(deployer_pubkey),
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

fn build_program_test(config: &ChainConfig<'_>, accounts: &ChainAccounts) -> ProgramTest {
    ensure_sbf_out_dir();

    let deployer_pubkey = config.deployer.pubkey();

    let mut pt = ProgramTest::new("ics26_router", ics26_router::ID, None);
    pt.add_program("mock_light_client", mock_light_client::ID, None);
    pt.add_program("access_manager", access_manager::ID, None);

    // ProgramData accounts for programs that verify upgrade authority
    add_program_data(&mut pt, access_manager::ID, deployer_pubkey);
    add_program_data(&mut pt, ics26_router::ID, deployer_pubkey);

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
                add_program_data(&mut pt, ics27_gmp::ID, deployer_pubkey);
            }
            Program::TestGmpApp => {
                pt.add_program("test_gmp_app", test_gmp_app::ID, None);
            }
            Program::TestCpiProxy => {
                pt.add_program("test_cpi_proxy", test_cpi_proxy::ID, None);
            }
            Program::Ift => {
                pt.add_program("ift", ift::ID, None);
                add_program_data(&mut pt, ift::ID, deployer_pubkey);
            }
            Program::TestAccessManager => {
                pt.add_program("test_access_manager", test_access_manager::ID, None);
                add_program_data(&mut pt, test_access_manager::ID, deployer_pubkey);
            }
        }
    }

    // Pre-fund deployer (admin and relayer are prefunded via chain.prefund())
    pt.add_account(deployer_pubkey, system_account(DEFAULT_PREFUND_LAMPORTS));

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
