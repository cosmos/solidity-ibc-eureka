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
            let config = Config::from_file(config)?;
            tracing::info!("Starting server with config: {:?}", config);
            let aggregator_service = AggregatorService::from_config(config.clone()).await?;

            start_server(aggregator_service, config).await?;

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Received Ctrl+C, shutting down server.");
                }
            }
        }
    }

    Ok(())
}
