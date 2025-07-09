use anchor_client::solana_client::rpc_client::RpcClient;
use anchor_client::solana_sdk::{
    commitment_config::CommitmentConfig,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use anchor_client::{Client, Cluster, Program};
use ics07_tendermint::{ClientState, ConsensusState};
use solana_system_interface::program as system_program;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct TestEnv {
    pub payer: Rc<Keypair>,
    pub client: Client<Rc<Keypair>>,
    pub program: Program<Rc<Keypair>>,
}

pub fn setup_test_env(program_id: Pubkey) -> TestEnv {
    let payer = Rc::new(Keypair::new());

    let client = Client::new_with_options(
        Cluster::Localnet,
        payer.clone(),
        CommitmentConfig::confirmed(),
    );

    let rpc = RpcClient::new_with_commitment(
        Cluster::Localnet.url().to_string(),
        CommitmentConfig::confirmed(),
    );

    let program = client.program(program_id).expect("Failed to get program");

    let env = TestEnv {
        payer,
        client,
        program,
    };

    let airdrop_sig = request_airdrop(&env, &rpc, 2 * LAMPORTS_PER_SOL);
    wait_for_airdrop_confirmation(&env, &rpc, &airdrop_sig, 30);

    env
}

pub fn log(setup: &TestEnv, message: &str) {
    println!("[payer: {}] {}", setup.payer.pubkey(), message);
}

fn request_airdrop(
    env: &TestEnv,
    rpc: &RpcClient,
    amount: u64,
) -> anchor_client::solana_sdk::signature::Signature {
    log(&env, &format!("💰 Requesting Airdrop - amount: {}", amount));

    let signature = rpc
        .request_airdrop(&env.payer.pubkey(), amount)
        .expect("Failed to request airdrop");

    log(&env, &format!("💰 Airdrop requested - sig: {}", signature));

    signature
}

fn wait_for_airdrop_confirmation(
    env: &TestEnv,
    rpc: &RpcClient,
    airdrop_sig: &anchor_client::solana_sdk::signature::Signature,
    max_attempts: u64,
) {
    let mut attempts = 0;
    while attempts < max_attempts {
        match rpc.confirm_transaction(airdrop_sig) {
            Ok(true) => {
                log(env, &format!("✅ Airdrop confirmed - sig: {}", airdrop_sig));
                return;
            }
            Ok(false) | Err(_) => {
                attempts += 1;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    }
    panic!(
        "Airdrop confirmation timeout after {} seconds",
        max_attempts
    );
}

pub struct InitializedContract {
    pub client_data_pda: Pubkey,
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
}

pub fn generate_unique_chain_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("test-chain-{}", timestamp)
}

pub fn initialize_contract(
    env: &TestEnv,
    program_id: Pubkey,
    client_state: ClientState,
    consensus_state: ConsensusState,
) -> InitializedContract {
    let (client_data_pda, _bump) =
        Pubkey::find_program_address(&[b"client", client_state.chain_id.as_bytes()], &program_id);

    log(
        &env,
        &format!(
            "🚀 Initializing contract with chain_id: {}",
            client_state.chain_id
        ),
    );
    log(&env, &format!("📍 Client data PDA: {}", client_data_pda));

    // Calculate the consensus state store PDA for the initial height (0)
    let (consensus_state_store, _bump) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_data_pda.as_ref(),
            &0u64.to_le_bytes(), // Initial height is 0 in our test client state
        ],
        &env.program.id(),
    );

    // Build and send the initialize instruction
    let instruction = env
        .program
        .request()
        .args(ics07_tendermint::instruction::Initialize {
            client_state: client_state.clone(),
            consensus_state: consensus_state.clone(),
        })
        .accounts(ics07_tendermint::accounts::Initialize {
            client_data: client_data_pda,
            consensus_state_store,
            payer: env.payer.pubkey(),
            system_program: system_program::ID,
        })
        .instructions()
        .expect("Failed to build instruction");

    let signature = env
        .program
        .request()
        .instruction(instruction[0].clone())
        .signer(env.payer.as_ref())
        .send()
        .expect("Failed to initialize contract");

    log(
        &env,
        &format!("✅ Contract initialized - tx: {}", signature),
    );

    InitializedContract {
        client_data_pda,
        client_state,
        consensus_state,
    }
}
