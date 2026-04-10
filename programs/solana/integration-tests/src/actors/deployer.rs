//! Deployer / upgrade-authority actor.
//!
//! Orchestrates the full program initialization sequence (access manager,
//! router, light client, IBC apps and program-specific steps) and
//! transfers upgrade authority to the access manager PDA afterwards.

use super::Actor;
use crate::admin::Admin;
use crate::chain::{Chain, ChainProgram, InitStepSigner, MOCK_LC_LATEST_HEIGHT};
use crate::relayer::Relayer;
use anchor_lang::InstructionData;
use solana_sdk::{
    bpf_loader_upgradeable,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    system_program,
    transaction::Transaction,
};

/// Deployer / upgrade-authority actor.
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
    /// Create a deployer with a fresh random keypair.
    pub fn new() -> Self {
        Self {
            keypair: Keypair::new(),
        }
    }

    /// Borrow the underlying keypair (used as upgrade authority).
    pub const fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    // ── Public init methods ──────────────────────────────────────────────

    /// Initialize the full IBC stack on the chain.
    ///
    /// Must be called after `chain.start()`. Executes AM, router, light
    /// client, IBC app registration and program-specific initialization
    /// transactions.
    pub async fn init_ibc_stack(
        &self,
        chain: &mut Chain,
        admin: &Admin,
        relayer: &Relayer,
        programs: &[&dyn ChainProgram],
    ) {
        let steps = build_init_steps(
            self.keypair(),
            admin.pubkey(),
            relayer.pubkey(),
            chain.client_id(),
            chain.counterparty_client_id(),
            programs,
        );

        let admin_kp = admin.keypair();
        for (ixs, signer) in &steps {
            let signers: Vec<&Keypair> = match signer {
                InitStepSigner::DeployerOnly => vec![self.keypair()],
                InitStepSigner::WithAdmin => vec![self.keypair(), admin_kp],
            };
            submit_tx(chain, ixs, &signers).await;
        }
    }

    /// Run only the program-specific initialization steps.
    ///
    /// Unlike [`init_ibc_stack`](Self::init_ibc_stack), this skips the core
    /// infrastructure (AM, router, light client, IBC app registration).
    /// Use this when a program needs a different admin than the core stack.
    pub async fn init_programs(
        &self,
        chain: &mut Chain,
        admin: Pubkey,
        programs: &[&dyn ChainProgram],
    ) {
        for p in programs {
            for (ixs, _signer) in p.init_steps(self.keypair(), admin) {
                submit_tx(chain, &ixs, &[self.keypair()]).await;
            }
        }
    }

    /// Transfer upgrade authority of all deployed programs to the access
    /// manager PDA, reflecting a production deployment where governance
    /// controls upgrades.
    pub async fn transfer_upgrade_authority(
        &self,
        chain: &mut Chain,
        programs: &[&dyn ChainProgram],
    ) {
        let am_pda = derive_access_manager_pda();
        let ixs = build_transfer_upgrade_authority_ixs(self.keypair().pubkey(), am_pda, programs);
        if !ixs.is_empty() {
            submit_tx(chain, &ixs, &[self.keypair()]).await;
        }
    }

    /// Register an additional client/counterparty pair on the chain.
    ///
    /// Initializes the mock light client and calls `add_client` on the
    /// router. Used for multi-hop tests (e.g. three-chain roundtrip).
    pub async fn add_counterparty(
        &self,
        chain: &mut Chain,
        admin: &Admin,
        client_id: &str,
        counterparty_client_id: &str,
    ) {
        let lc_ix = build_mock_lc_initialize_ix(self.keypair().pubkey(), client_id);
        submit_tx(chain, &[lc_ix], &[self.keypair()]).await;

        let add_ix = build_add_client_ix(
            admin.pubkey(),
            derive_router_state_pda(),
            derive_access_manager_pda(),
            client_id,
            counterparty_client_id,
        );
        submit_tx(chain, &[add_ix], &[self.keypair(), admin.keypair()]).await;
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

async fn submit_tx(chain: &mut Chain, ixs: &[Instruction], signers: &[&Keypair]) {
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&signers[0].pubkey()),
        signers,
        chain.blockhash(),
    );
    chain.process_transaction(tx).await.expect("init tx failed");
}

// ── PDA derivation ──────────────────────────────────────────────────────

fn derive_access_manager_pda() -> Pubkey {
    solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID).0
}

fn derive_router_state_pda() -> Pubkey {
    Pubkey::find_program_address(&[ics26_router::state::RouterState::SEED], &ics26_router::ID).0
}

// ── Init step builder ───────────────────────────────────────────────────

/// Return the (`port_id`, `program_id`) for the first IBC application in the list.
fn get_port_and_app<'a>(programs: &'a [&'a dyn ChainProgram]) -> (&'a str, Pubkey) {
    for p in programs {
        if let Some(port_and_id) = p.ibc_port_and_id() {
            return port_and_id;
        }
    }
    panic!("no IBC application in programs list");
}

/// Build all initialization instructions grouped by transaction.
///
/// Returns `(instructions, signer)` tuples that must be executed
/// sequentially. The deployer always pays transaction fees; `WithAdmin`
/// steps additionally require the admin keypair as co-signer.
fn build_init_steps(
    deployer: &Keypair,
    admin_pubkey: Pubkey,
    relayer_pubkey: Pubkey,
    client_id: &str,
    counterparty_client_id: &str,
    programs: &[&dyn ChainProgram],
) -> Vec<(Vec<Instruction>, InitStepSigner)> {
    let deployer_pubkey = deployer.pubkey();
    let am_pda = derive_access_manager_pda();
    let router_state_pda = derive_router_state_pda();
    let (port_id, app_program_id) = get_port_and_app(programs);

    let mut steps = vec![
        // TX1: access_manager::initialize (deployer = upgrade authority + payer)
        (
            vec![build_am_initialize_ix(
                deployer_pubkey,
                deployer,
                am_pda,
                admin_pubkey,
            )],
            InitStepSigner::DeployerOnly,
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
            InitStepSigner::WithAdmin,
        ),
        // TX3: ics26_router::initialize (deployer = upgrade authority + payer)
        (
            vec![build_router_initialize_ix(
                deployer_pubkey,
                deployer,
                router_state_pda,
                access_manager::ID,
            )],
            InitStepSigner::DeployerOnly,
        ),
        // TX4: mock_light_client::initialize
        (
            vec![build_mock_lc_initialize_ix(deployer_pubkey, client_id)],
            InitStepSigner::DeployerOnly,
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
                    deployer_pubkey,
                    admin_pubkey,
                    router_state_pda,
                    am_pda,
                    port_id,
                    app_program_id,
                ),
            ],
            InitStepSigner::WithAdmin,
        ),
    ];

    // App-specific initialization (each program provides its own steps)
    for p in programs {
        steps.extend(p.init_steps(deployer, admin_pubkey));
    }

    steps
}

// ── Upgrade authority transfer ──────────────────────────────────────────

fn build_transfer_upgrade_authority_ixs(
    deployer: Pubkey,
    am_pda: Pubkey,
    programs: &[&dyn ChainProgram],
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
        if let Some(program_id) = p.upgrade_authority_program_id() {
            ixs.push(bpf_loader_upgradeable::set_upgrade_authority(
                &program_id,
                &deployer,
                Some(&am_pda),
            ));
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
    Instruction {
        program_id: access_manager::ID,
        accounts: vec![
            AccountMeta::new(am_pda, false),
            AccountMeta::new_readonly(admin, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: access_manager::instruction::GrantRole { role_id, account }.data(),
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
