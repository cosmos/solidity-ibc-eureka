use crate::chain::Chain;
use solana_program_test::BanksClientError;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signature::Keypair, signer::Signer,
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

/// Sign and submit a transaction with `chain.payer()` as fee payer and
/// `keypair` as the admin signer.
async fn send_admin_tx(
    keypair: &Keypair,
    chain: &mut Chain,
    ixs: &[Instruction],
) -> Result<(), BanksClientError> {
    let tx = Transaction::new_signed_with_payer(
        ixs,
        Some(&chain.payer().pubkey()),
        &[chain.payer(), keypair],
        chain.blockhash(),
    );
    chain.process_transaction(tx).await
}
