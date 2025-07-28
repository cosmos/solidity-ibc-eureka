use anyhow::Result;
use clap::Parser;
use sig_aggregator::{
    aggregator::AggregatorService,
    cli::{Cli, Commands},
    config::Config,
    server::start as start_server,
};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Server { config } => {
            let config = Config::from_file(config)?;

            let subscriber = FmtSubscriber::builder()
                .with_max_level(config.server.log_level())
                .finish();
            tracing::subscriber::set_global_default(subscriber)?;

            tracing::info!(
                "Starting sig-aggregator with attestor endpoints: {:?}",
                config.attestor.attestor_endpoints
            );

            let aggregator_service =
                AggregatorService::from_attestor_config(config.attestor).await?;
            start_server(aggregator_service, config.server).await?;
        }
    }

    Ok(())
}
