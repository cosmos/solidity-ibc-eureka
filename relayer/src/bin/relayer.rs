use clap::Parser;
use solidity_ibc_eureka_relayer::cli::cmd::{Commands, RelayerCli};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = RelayerCli::parse();
    match cli.command {
        Commands::Start(_args) => todo!(),
    }
}
