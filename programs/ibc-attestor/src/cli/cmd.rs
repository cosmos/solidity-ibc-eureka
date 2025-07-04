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

    /// The subcommand to run key management program.
    #[command(subcommand)]
    Key(key::KeyCommands),
}

/// The arguments for the start subcommand.
pub mod server {
    use super::Parser;

    /// The subcommands for the attestor.
    #[derive(Clone, Debug, Parser)]
    pub enum ServerKind {
        #[cfg(feature = "sol")]
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

/// The arguments for the start subcommand.
pub mod key {
    use super::Parser;

    /// The subcommands for the attestor.
    #[derive(Clone, Debug, Parser)]
    pub enum KeyCommands {
        /// The subcommand to generate a key pair at `...`
        Generate,
        /// The subcommand to show your private and public keys
        Show,
    }
}
