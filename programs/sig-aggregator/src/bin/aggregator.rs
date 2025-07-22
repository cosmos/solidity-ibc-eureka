use anyhow::Result;
use clap::Parser;
use sig_aggregator::{
    aggregator::AggregatorService,
    cli::{Cli, Commands},
    config::Config,
    server::start as start_server,
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

            let config = Config::from_file(config)
                .map_err(|e| anyhow::anyhow!("Configuration error: {}", e))?;

            // Validate configuration before proceeding
            config.validate()
                .map_err(|e| anyhow::anyhow!("Invalid configuration: {}", e))?;

            let aggregator_service = AggregatorService::from_config(config.attestor).await
                .map_err(|e| anyhow::anyhow!("Failed to initialize aggregator service: {e}"))?;

            start_server(aggregator_service, config.server).await
                .map_err(|e| anyhow::anyhow!("Server error: {e}"))?;
        }
    }

    Ok(())
}
