use anyhow::Result;
use clap::{Parser, Subcommand};

use attestor::key::{generate_and_store_key_pair, show_public_key};

#[derive(Parser, Debug)]
#[command(author, version, about = "Attestor CLI for managing keys", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generates a new secp256k1 key pair and stores it.
    Generate,
    /// Shows the existing public key.
    Show,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate => generate_and_store_key_pair()?,
        Commands::Show => show_public_key()?,
    }

    Ok(())
}
