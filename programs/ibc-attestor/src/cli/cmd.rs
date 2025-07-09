//! Defines the client interface for the attestor server.
use clap::{command, Parser};

/// The command line interface for the attestor.
#[derive(Clone, Debug, Parser)]
#[command(version, about, long_about = None)]
pub struct AttestorCli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// The subcommands for the attestor.
#[derive(Clone, Debug, Parser)]
pub enum Commands {
    /// The subcommand to run the attestor.
    #[command(subcommand)]
    Server(server::ServerKind),
}

/// The arguments for the start subcommand.
pub mod server {
    use super::Parser;

    /// The subcommands for the attestor.
    #[derive(Clone, Debug, Parser)]
    pub enum ServerKind {
        /// The subcommand to run the solana attestor
        Solana(Args),
    }

    /// The arguments for the start subcommand.
    #[derive(Clone, Debug, Parser)]
    pub struct Args {
        /// The configuration file for the attestor.
        #[clap(long)]
        pub config: String,
    }
}
