//! Defines the client interface for the attestor server.
use clap::{command, Parser};

#[derive(Clone, Debug, Parser)]
#[command(
    name = "ibc_attestor",
    version,
    about = "IBC Attestor - Blockchain state attestation service",
    long_about = "A service for generating cryptographic attestations of blockchain state.\nSupports key management and running attestation servers."
)]
/// The command line interface for the attestor.
pub struct AttestorCli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Commands,
}

/// The subcommands for the attestor.
#[derive(Clone, Debug, Parser)]
pub enum Commands {
    /// The subcommand to run the server.
    Server(server::Args),

    /// The subcommand to run key management program.
    #[command(subcommand)]
    Key(key::KeyCommands),
}

/// The arguments for the start subcommand.
pub mod server {
    use super::Parser;

    /// The arguments for the server subcommand.
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

    #[derive(Clone, Debug, Parser)]
    pub enum KeyCommands {
        Generate,
        Show(ShowArgs),
    }

    #[derive(Clone, Debug, Parser)]
    pub struct ShowArgs {
        #[clap(long)]
        pub hide_private: bool,
        #[clap(long)]
        pub hide_public: bool,
    }
}
