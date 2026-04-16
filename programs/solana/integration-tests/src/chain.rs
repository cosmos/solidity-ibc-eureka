//! Simulated Solana chain backed by `ProgramTest` / `BanksClient`.
//!
//! Handles program registration, deterministic clock setup, actor funding,
//! transaction submission and account reads. Each [`Chain`] instance
//! represents an independent local validator used by a single test.

use crate::actors::admin::Admin;
use crate::actors::deployer::Deployer;
use crate::actors::relayer::Relayer;
use crate::Actor;
use solana_program_test::{BanksClient, BanksClientError, ProgramTest};
use solana_sdk::{
    account::Account,
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    hash::Hash,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::Keypair,
    system_program,
    sysvar::Sysvar as _,
    transaction::Transaction,
};

const DEPLOY_DIR: &str = "../target/deploy";
/// Deterministic Unix timestamp used for every test chain clock.
pub const TEST_CLOCK_TIME: i64 = 1_700_000_000;
const DEFAULT_PREFUND_LAMPORTS: u64 = 10_000_000_000;
pub(crate) const MOCK_LC_LATEST_HEIGHT: u64 = 1;

// ── Light-client account references ─────────────────────────────────────

/// Light-client account references passed to router instruction builders.
///
/// Decouples instruction construction from the specific light client
/// implementation (mock LC vs attestation LC).
#[derive(Clone)]
pub struct LcAccounts {
    pub program_id: Pubkey,
    pub client_state: Pubkey,
    pub consensus_state: Pubkey,
}

/// Derive mock light client [`LcAccounts`] for a given `client_id`.
pub fn mock_lc_accounts(client_id: &str) -> LcAccounts {
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
    LcAccounts {
        program_id: mock_light_client::ID,
        client_state,
        consensus_state,
    }
}

/// Derive attestation light client [`LcAccounts`] for a given program and proof height.
///
/// Unlike mock LC, the attestation client state PDA is height-independent
/// (seeds: `["client"]`), and the consensus state PDA depends on the proof
/// height (seeds: `["consensus_state", height.to_le_bytes()]`).
pub fn attestation_lc_accounts(program_id: Pubkey, proof_height: u64) -> LcAccounts {
    let (client_state, _) = Pubkey::find_program_address(&[b"client"], &program_id);
    let (consensus_state, _) = Pubkey::find_program_address(
        &[b"consensus_state", &proof_height.to_le_bytes()],
        &program_id,
    );
    LcAccounts {
        program_id,
        client_state,
        consensus_state,
    }
}

/// Deterministic PDA for `mock_ibc_app`'s dummy state account.
///
/// `mock_ibc_app` has no `initialize` instruction, so we pre-create a dummy
/// account at this address during `ProgramTest` setup. The same PDA must be
/// used everywhere the router needs the app's state account.
pub fn mock_ibc_app_state_pda() -> Pubkey {
    Pubkey::find_program_address(&[b"state"], &mock_ibc_app::ID).0
}

// ── ChainProgram trait ──────────────────────────────────────────────────

/// Which signers a particular init step requires.
///
/// The deployer always pays transaction fees and signs as the fee payer.
/// `WithAdmin` adds the admin keypair as a co-signer for AM-role-gated steps.
#[derive(Clone, Copy)]
pub enum InitStepSigner {
    DeployerOnly,
    WithAdmin,
}

/// A program that can be loaded onto a test chain.
///
/// Each implementor knows how to register itself on `ProgramTest`, provide
/// IBC port info and build its own initialization / upgrade-authority steps.
pub trait ChainProgram: Sync {
    /// Register this program on a `ProgramTest` instance.
    fn register(&self, pt: &mut ProgramTest, deployer: Pubkey);

    /// Return `(port_id, program_id)` if this program is an IBC application
    /// that should be registered on the router via `add_ibc_app`.
    fn ibc_port_and_id(&self) -> Option<(&str, Pubkey)> {
        None
    }

    /// Return program-specific initialization steps (run after the common
    /// AM + router + light-client + `add_client` + `add_ibc_app` transactions).
    fn init_steps(
        &self,
        _deployer: &Keypair,
        _admin: Pubkey,
    ) -> Vec<(Vec<Instruction>, InitStepSigner)> {
        vec![]
    }

    /// Return the program ID whose upgrade authority should be transferred
    /// to the access manager PDA after initialization.
    fn upgrade_authority_program_id(&self) -> Option<Pubkey> {
        None
    }
}

// ── Chain config & runtime ──────────────────────────────────────────────

/// Parameters for constructing a new [`Chain`].
pub struct ChainConfig<'a> {
    /// Light-client identifier on this chain (e.g. `"08-wasm-0"`).
    pub client_id: &'a str,
    /// Light-client identifier on the counterparty chain.
    pub counterparty_client_id: &'a str,
    /// Deployer actor that holds upgrade authority.
    pub deployer: &'a Deployer,
    /// Application programs to register alongside the core stack.
    pub programs: &'a [&'a dyn ChainProgram],
    /// Light-client program ID used for `add_client`. Defaults to mock LC.
    pub lc_program_id: Pubkey,
}

/// Simulated Solana validator for a single test.
///
/// Has two phases: **setup** (before [`Chain::start`]) where programs and
/// accounts are registered, and **runtime** (after start) where
/// transactions can be submitted.
pub struct Chain {
    pt: Option<ProgramTest>,
    client_id: String,
    counterparty_client_id: String,
    clock_time: i64,
    banks: Option<BanksClient>,
    blockhash: Hash,
    lc_program_id: Pubkey,
}

impl Chain {
    /// Create a new chain from the given config (setup phase).
    pub fn new(config: ChainConfig<'_>) -> Self {
        let lc_program_id = config.lc_program_id;
        let pt = build_program_test(&config);

        Self {
            pt: Some(pt),
            client_id: config.client_id.to_string(),
            counterparty_client_id: config.counterparty_client_id.to_string(),
            clock_time: TEST_CLOCK_TIME,
            banks: None,
            blockhash: Hash::default(),
            lc_program_id,
        }
    }

    /// Create a single chain with the default `chain-a-client` /
    /// `chain-b-client` client IDs and mock light client.
    pub fn single(deployer: &Deployer, programs: &[&dyn ChainProgram]) -> Self {
        Self::new(ChainConfig {
            client_id: "chain-a-client",
            counterparty_client_id: "chain-b-client",
            deployer,
            programs,
            lc_program_id: mock_light_client::ID,
        })
    }

    /// Create a single chain with a specific light-client program.
    pub fn single_with_lc(
        deployer: &Deployer,
        programs: &[&dyn ChainProgram],
        lc_program_id: Pubkey,
    ) -> Self {
        Self::new(ChainConfig {
            client_id: "chain-a-client",
            counterparty_client_id: "chain-b-client",
            deployer,
            programs,
            lc_program_id,
        })
    }

    /// Create two chains with mirrored client IDs and the same programs.
    pub fn pair(deployer: &Deployer, programs: &[&dyn ChainProgram]) -> (Self, Self) {
        Self::pair_with(deployer, programs, programs)
    }

    /// Create two chains with mirrored client IDs but different programs.
    pub fn pair_with(
        deployer: &Deployer,
        programs_a: &[&dyn ChainProgram],
        programs_b: &[&dyn ChainProgram],
    ) -> (Self, Self) {
        Self::pair_with_lc(deployer, programs_a, programs_b, mock_light_client::ID)
    }

    /// Create two chains with mirrored client IDs, different programs and a
    /// specific light-client program.
    pub fn pair_with_lc(
        deployer: &Deployer,
        programs_a: &[&dyn ChainProgram],
        programs_b: &[&dyn ChainProgram],
        lc_program_id: Pubkey,
    ) -> (Self, Self) {
        let chain_a = Self::new(ChainConfig {
            client_id: "chain-a-client",
            counterparty_client_id: "chain-b-client",
            deployer,
            programs: programs_a,
            lc_program_id,
        });
        let chain_b = Self::new(ChainConfig {
            client_id: "chain-b-client",
            counterparty_client_id: "chain-a-client",
            deployer,
            programs: programs_b,
            lc_program_id,
        });
        (chain_a, chain_b)
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
    /// After calling `start()`, use `Deployer::init_ibc_stack` and
    /// `Deployer::transfer_upgrade_authority` to initialize on-chain state.
    pub async fn start(&mut self) {
        let pt = self.pt.take().expect("chain already started");
        let (banks, _payer, blockhash) = pt.start().await;
        self.banks = Some(banks);
        self.blockhash = blockhash;
    }

    /// Start the chain and initialize the full IBC stack, transferring
    /// upgrade authority to the access manager PDA.
    ///
    /// Combines `start`, `Deployer::init_ibc_stack` and
    /// `Deployer::transfer_upgrade_authority` into the canonical setup
    /// sequence used by most tests.
    pub async fn init(
        &mut self,
        deployer: &Deployer,
        admin: &Admin,
        relayer: &Relayer,
        programs: &[&dyn ChainProgram],
    ) {
        self.start().await;
        deployer
            .init_ibc_stack(self, admin, relayer, programs)
            .await;
        deployer.transfer_upgrade_authority(self, programs).await;
    }

    /// Like [`init`](Self::init) but also submits an `update_client`
    /// transaction for the attestation LC, creating a consensus state at
    /// [`PROOF_HEIGHT`](crate::router::PROOF_HEIGHT).
    ///
    /// Attestation LC requires an explicit `update_client` after
    /// initialization (unlike mock LC which is pre-seeded with a
    /// consensus state). This method bundles both steps so attestation
    /// tests don't need a separate `update_client` call.
    pub async fn init_with_attestation(
        &mut self,
        deployer: &Deployer,
        admin: &Admin,
        relayer: &Relayer,
        programs: &[&dyn ChainProgram],
        attestors: &crate::attestor::Attestors,
    ) {
        self.init(deployer, admin, relayer, programs).await;
        relayer
            .attestation_update_client(self, attestors, crate::router::PROOF_HEIGHT)
            .await
            .expect("attestation update_client during init failed");
    }

    // ── Runtime phase (after start) ─────────────────────────────────────

    /// Light-client identifier on this chain.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Light-client identifier on the counterparty chain.
    pub fn counterparty_client_id(&self) -> &str {
        &self.counterparty_client_id
    }

    /// Deterministic Unix timestamp set during chain construction.
    pub const fn clock_time(&self) -> i64 {
        self.clock_time
    }

    /// The light-client program ID registered on this chain.
    pub const fn lc_program_id(&self) -> Pubkey {
        self.lc_program_id
    }

    /// Default [`LcAccounts`] for this chain's primary client.
    ///
    /// For mock LC this is always valid. For attestation LC, the consensus
    /// state PDA uses the latest height stored in the client — callers that
    /// need a specific proof height should use [`lc_accounts_at_height`].
    pub fn lc_accounts(&self) -> LcAccounts {
        if self.lc_program_id == mock_light_client::ID {
            mock_lc_accounts(&self.client_id)
        } else {
            attestation_lc_accounts(self.lc_program_id, crate::router::PROOF_HEIGHT)
        }
    }

    /// [`LcAccounts`] for a specific proof height (attestation LC only).
    ///
    /// For mock LC this returns the same accounts regardless of height.
    pub fn lc_accounts_at_height(&self, height: u64) -> LcAccounts {
        if self.lc_program_id == mock_light_client::ID {
            mock_lc_accounts(&self.client_id)
        } else {
            attestation_lc_accounts(self.lc_program_id, height)
        }
    }

    /// Derive the `test_gmp_app` `CounterAppState` PDA.
    pub fn counter_app_state_pda(&self) -> Pubkey {
        Pubkey::find_program_address(
            &[test_gmp_app::state::CounterAppState::SEED],
            &test_gmp_app::ID,
        )
        .0
    }

    /// Derive the IFT `IFTAppState` PDA.
    pub fn ift_app_state_pda(&self) -> Pubkey {
        Pubkey::find_program_address(&[ift::constants::IFT_APP_STATE_SEED], &ift::ID).0
    }

    /// Latest blockhash, refreshed after every transaction.
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

// ── Internal ProgramTest builder ────────────────────────────────────────

fn ensure_sbf_out_dir() {
    if std::env::var("SBF_OUT_DIR").is_err() {
        std::env::set_var("SBF_OUT_DIR", std::path::Path::new(DEPLOY_DIR));
    }
}

pub(crate) fn add_program_data(pt: &mut ProgramTest, program_id: Pubkey, deployer_pubkey: Pubkey) {
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

fn build_program_test(config: &ChainConfig<'_>) -> ProgramTest {
    ensure_sbf_out_dir();

    let deployer_pubkey = config.deployer.pubkey();

    let mut pt = ProgramTest::new("ics26_router", ics26_router::ID, None);
    pt.add_program("access_manager", access_manager::ID, None);

    // ProgramData accounts for programs that verify upgrade authority
    add_program_data(&mut pt, access_manager::ID, deployer_pubkey);
    add_program_data(&mut pt, ics26_router::ID, deployer_pubkey);

    // Register mock LC by default; attestation LC is registered via ChainProgram
    if config.lc_program_id == mock_light_client::ID {
        pt.add_program("mock_light_client", mock_light_client::ID, None);
    }

    for program in config.programs {
        program.register(&mut pt, deployer_pubkey);
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
