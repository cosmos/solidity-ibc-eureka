use anyhow::Result;
use clap::Parser;
use sig_aggregator::{
    aggregator::AggregatorService,
    cli::{Cli, Commands},
    config::Config,
    server::Server,
};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Server { config } => {
            let config = Config::load(config)?;
            let aggregator_service = AggregatorService::from_config(config.clone()).await?;

            let server = Server;
            server.start(aggregator_service, config).await?;
        }
    }

    Ok(())
}
