use crate::chain::Chain;
use solana_program_test::BanksClientError;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer as _,
    transaction::Transaction,
};

pub mod admin;
pub mod deployer;
pub mod ift_admin;
pub mod relayer;
pub mod user;

/// Shared interface for test actors (`Deployer`, `Admin`, `IftAdmin`, `User`, `Relayer`).
pub trait Actor {
    fn pubkey(&self) -> Pubkey;
}

/// Sign and submit a transaction with `keypair` as both fee payer and signer.
async fn send_tx(
    keypair: &Keypair,
    chain: &mut Chain,
    ixs: &[Instruction],
) -> Result<(), BanksClientError> {
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&keypair.pubkey()),
        &[keypair],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await
}
