use crate::accounts::*;
use crate::relayer::Relayer;
use crate::Actor;
use solana_program_test::{BanksClient, BanksClientError, ProgramTest};
use solana_sdk::{
    account::Account, hash::Hash, pubkey::Pubkey, signature::Keypair, system_program,
    sysvar::Sysvar as _,
};

const DEPLOY_DIR: &str = "../target/deploy";
pub const TEST_CLOCK_TIME: i64 = 1_700_000_000;
const DEFAULT_PREFUND_LAMPORTS: u64 = 10_000_000_000;

// ── Account metadata ────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub struct ChainAccounts {
    pub mock_client_state: Pubkey,
    pub mock_consensus_state: Pubkey,
    pub app_state_pda: Pubkey,
    pub gmp_app_state_pda: Option<Pubkey>,
    pub counter_app_state_pda: Option<Pubkey>,
}

pub struct ChainConfig<'a> {
    pub client_id: &'a str,
    pub counterparty_client_id: &'a str,
    pub relayer: &'a Relayer,
    pub clock_time: i64,
    pub include_gmp: bool,
}

// ── Chain (setup + runtime) ─────────────────────────────────────────────

pub struct Chain {
    pt: Option<ProgramTest>,
    client_id: String,
    counterparty_client_id: String,
    clock_time: i64,
    banks: Option<BanksClient>,
    payer: Option<Keypair>,
    blockhash: Hash,
    pub accounts: ChainAccounts,
}

impl Chain {
    pub fn new(config: ChainConfig<'_>) -> Self {
        let (pt, accounts) = build_program_test(&config);
        Self {
            pt: Some(pt),
            client_id: config.client_id.to_string(),
            counterparty_client_id: config.counterparty_client_id.to_string(),
            clock_time: config.clock_time,
            banks: None,
            payer: None,
            blockhash: Hash::default(),
            accounts,
        }
    }

    // ── Setup phase (before start) ──────────────────────────────────────

    /// Pre-fund an actor's account with the default amount (10 SOL).
    pub fn prefund(&mut self, actor: &impl Actor) {
        self.prefund_lamports(actor.pubkey(), DEFAULT_PREFUND_LAMPORTS);
    }

    /// Pre-fund an account with a specific lamport amount.
    pub fn prefund_lamports(&mut self, pubkey: Pubkey, lamports: u64) {
        self.pt().add_account(
            pubkey,
            Account {
                lamports,
                data: vec![],
                owner: system_program::ID,
                executable: false,
                rent_epoch: 0,
            },
        );
    }

    /// Start the chain runtime.
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

    pub const fn clock_time(&self) -> i64 {
        self.clock_time
    }

    pub const fn payer(&self) -> &Keypair {
        self.payer.as_ref().expect("chain not started yet")
    }

    pub const fn blockhash(&self) -> Hash {
        self.blockhash
    }

    /// Submit a transaction and auto-refresh the blockhash.
    pub async fn process_transaction(
        &mut self,
        tx: solana_sdk::transaction::Transaction,
    ) -> Result<(), BanksClientError> {
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

// ── Internal ProgramTest builder ────────────────────────────────────────

fn ensure_sbf_out_dir() {
    if std::env::var("SBF_OUT_DIR").is_err() {
        std::env::set_var("SBF_OUT_DIR", std::path::Path::new(DEPLOY_DIR));
    }
}

fn build_program_test(config: &ChainConfig<'_>) -> (ProgramTest, ChainAccounts) {
    ensure_sbf_out_dir();

    let mut pt = ProgramTest::new("ics26_router", ics26_router::ID, None);
    pt.add_program("mock_light_client", mock_light_client::ID, None);
    pt.add_program("access_manager", access_manager::ID, None);

    // RouterState
    let (pda, data) = setup_router_state();
    pt.add_account(pda, account_owned_by(data, ics26_router::ID));

    // AccessManager with RELAYER_ROLE
    let (pda, data) = setup_access_manager_with_roles(&[(
        solana_ibc_types::roles::RELAYER_ROLE,
        &[config.relayer.pubkey()],
    )]);
    pt.add_account(pda, account_owned_by(data, access_manager::ID));

    // Client
    let (pda, data) = setup_client(
        config.client_id,
        mock_light_client::ID,
        config.counterparty_client_id,
        true,
    );
    pt.add_account(pda, account_owned_by(data, ics26_router::ID));

    // Mock light client state
    let mock_client_state = Pubkey::new_unique();
    pt.add_account(
        mock_client_state,
        account_owned_by(vec![0u8; 64], mock_light_client::ID),
    );
    let mock_consensus_state = Pubkey::new_unique();
    pt.add_account(
        mock_consensus_state,
        account_owned_by(vec![0u8; 64], mock_light_client::ID),
    );

    // Pre-fund relayer
    pt.add_account(
        config.relayer.pubkey(),
        Account {
            lamports: DEFAULT_PREFUND_LAMPORTS,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        },
    );

    // Deterministic clock
    let clock = solana_sdk::clock::Clock {
        slot: 1,
        epoch_start_timestamp: config.clock_time,
        epoch: 0,
        leader_schedule_epoch: 0,
        unix_timestamp: config.clock_time,
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

    let (app_state_pda, gmp_app_state_pda, counter_app_state_pda) = if config.include_gmp {
        setup_gmp_chain(&mut pt)
    } else {
        setup_router_only_chain(&mut pt)
    };

    (
        pt,
        ChainAccounts {
            mock_client_state,
            mock_consensus_state,
            app_state_pda,
            gmp_app_state_pda,
            counter_app_state_pda,
        },
    )
}

fn setup_router_only_chain(pt: &mut ProgramTest) -> (Pubkey, Option<Pubkey>, Option<Pubkey>) {
    use crate::router::PORT_ID;

    pt.add_program("test_ibc_app", test_ibc_app::ID, None);

    let (pda, data) = setup_ibc_app(PORT_ID, test_ibc_app::ID);
    pt.add_account(pda, account_owned_by(data, ics26_router::ID));

    let (app_state_pda, _) =
        Pubkey::find_program_address(&[solana_ibc_types::IBCAppState::SEED], &test_ibc_app::ID);
    let app_state = test_ibc_app::state::TestIbcAppState {
        authority: Pubkey::new_unique(),
        packets_received: 0,
        packets_acknowledged: 0,
        packets_timed_out: 0,
        packets_sent: 0,
    };
    pt.add_account(
        app_state_pda,
        account_owned_by(create_account_data(&app_state), test_ibc_app::ID),
    );

    (app_state_pda, None, None)
}

fn setup_gmp_chain(pt: &mut ProgramTest) -> (Pubkey, Option<Pubkey>, Option<Pubkey>) {
    pt.add_program("ics27_gmp", ics27_gmp::ID, None);
    pt.add_program("test_gmp_app", test_gmp_app::ID, None);

    let (pda, data) = setup_ibc_app(crate::gmp::GMP_PORT_ID, ics27_gmp::ID);
    pt.add_account(pda, account_owned_by(data, ics26_router::ID));

    let (gmp_app_state_pda, gmp_bump) =
        Pubkey::find_program_address(&[ics27_gmp::state::GMPAppState::SEED], &ics27_gmp::ID);
    let (_, gmp_data) = setup_gmp_app_state(gmp_bump, false);
    pt.add_account(gmp_app_state_pda, account_owned_by(gmp_data, ics27_gmp::ID));

    let (counter_pda, counter_bump) = Pubkey::find_program_address(
        &[test_gmp_app::state::CounterAppState::SEED],
        &test_gmp_app::ID,
    );
    let (_, counter_data) = setup_counter_app_state(counter_bump, Pubkey::new_unique());
    pt.add_account(
        counter_pda,
        account_owned_by(counter_data, test_gmp_app::ID),
    );

    (
        gmp_app_state_pda,
        Some(gmp_app_state_pda),
        Some(counter_pda),
    )
}
