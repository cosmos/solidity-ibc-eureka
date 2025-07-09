use anchor_client::{
    solana_sdk::{
        commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Keypair, signer::Signer,
        system_program,
    },
    Client, Cluster, Program,
};
use ics07_tendermint::{ClientState, ConsensusState};
use std::str::FromStr;

pub fn setup_test_environment() -> (Pubkey, Keypair, Keypair) {
    let program_id = Pubkey::from_str("8wQAC7oWLTxExhR49jYAzXZB39mu7WVVvkWJGgAMMjpV").unwrap();
    let payer = Keypair::new();
    let client_data = Keypair::new();
    (program_id, payer, client_data)
}

pub fn create_client(payer: &Keypair) -> Client<&Keypair> {
    Client::new_with_options(Cluster::Localnet, payer, CommitmentConfig::confirmed())
}

pub fn create_test_client_state() -> ClientState {
    ClientState {
        chain_id: "test-chain".to_string(),
        trust_level_numerator: 1,
        trust_level_denominator: 3,
        trusting_period: 1209600,
        unbonding_period: 1814400,
        max_clock_drift: 5,
        frozen_height: 0,
        latest_height: 1,
    }
}

pub fn create_test_consensus_state() -> ConsensusState {
    ConsensusState {
        timestamp: 1234567890,
        root: [0u8; 32],
        next_validators_hash: [1u8; 32],
    }
}

pub fn initialize_test_client(
    program: &Program<&Keypair>,
    client_data: &Keypair,
    payer: &Keypair,
) -> Result<String, Box<dyn std::error::Error>> {
    let client_state = create_test_client_state();
    let consensus_state = create_test_consensus_state();

    // Calculate the consensus state store PDA for the initial height (1)
    let (consensus_state_store, _bump) = Pubkey::find_program_address(
        &[
            b"consensus_state",
            client_data.pubkey().as_ref(),
            &1u64.to_le_bytes(), // Initial height is 1 in our test client state
        ],
        &program.id(),
    );

    let result = program
        .request()
        .accounts(ics07_tendermint::accounts::Initialize {
            client_data: client_data.pubkey(),
            consensus_state_store,
            payer: payer.pubkey(),
            system_program: system_program::id(),
        })
        .args(ics07_tendermint::instruction::Initialize {
            client_state,
            consensus_state,
        })
        .signer(client_data)
        .send()?;

    Ok(result.to_string())
}

pub fn load_program_or_fail<'a>(
    client: &'a Client<&'a Keypair>,
    program_id: Pubkey,
) -> Result<Program<&'a Keypair>, Box<dyn std::error::Error>> {
    match client.program(program_id) {
        Ok(program) => Ok(program),
        Err(e) => {
            println!("❌ Failed to load program: {}", e);
            println!("💡 Make sure the program is deployed with: anchor deploy");
            Err(Box::new(e))
        }
    }
}

pub fn with_initialized_client<F>(test_name: &str, test_fn: F)
where
    F: FnOnce(&Program<&Keypair>, &Keypair) -> Result<(), Box<dyn std::error::Error>>,
{
    println!("🧪 Testing ICS07 Tendermint client {} function", test_name);

    let (program_id, payer, client_data) = setup_test_environment();
    let client = create_client(&payer);

    // Fund the payer account with SOL for transaction fees
    let rpc_url = "http://localhost:8899";
    let rpc_client = anchor_client::solana_client::rpc_client::RpcClient::new(rpc_url);
    rpc_client
        .request_airdrop(&payer.pubkey(), 10_000_000_000) // 10 SOL
        .and_then(|sig| rpc_client.confirm_transaction(&sig))
        .unwrap_or_else(|e| panic!("Failed to fund payer for {} test: {}", test_name, e));
    println!("💰 Airdropped 10 SOL to payer");

    let program = load_program_or_fail(&client, program_id)
        .unwrap_or_else(|_| panic!("Failed to load program for {} test", test_name));

    let signature = initialize_test_client(&program, &client_data, &payer)
        .unwrap_or_else(|e| panic!("Failed to initialize client for {} test: {}", test_name, e));

    println!(
        "✅ Initialize successful: {}, now testing {}",
        signature, test_name
    );

    test_fn(&program, &client_data).unwrap_or_else(|e| panic!("{} test failed: {}", test_name, e));

    println!("✅ {} test successful", test_name);
    println!("🎯 {} test completed!", test_name);
}
