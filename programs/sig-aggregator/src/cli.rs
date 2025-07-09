use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Run the aggregator service
    Aggregator {
        #[arg(long, default_value = "config.toml")]
        config: String,
    },
    /// Run a relayer client to query the aggregator
    Relayer {
        #[arg(long)]
        aggregator_addr: String,
        #[arg(long)]
        min_height: u64,
    },
}
