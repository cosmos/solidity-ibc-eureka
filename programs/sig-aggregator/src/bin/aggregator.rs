use anyhow::Result;
use clap::Parser;
use sig_aggregator::{
    aggregator::Aggregator,
    cli::{Cli, Commands},
    config::Config,
    server::start as start_server,
};
use tracing_subscriber::FmtSubscriber;

fn init_logging(log_level: tracing::Level) -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .finish();

    tracing::subscriber::set_global_default(subscriber).map_err(Into::into)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let Commands::Server { config } = cli.command;

    let config = Config::from_file(config)?;
    init_logging(config.server.log_level())?;

    tracing::info!(
        "Starting sig-aggregator with attestor endpoints: {:?}",
        config.attestor.attestor_endpoints
    );

    let aggregator = Aggregator::from_attestor_config(config.attestor).await?;
    start_server(aggregator, config.server).await
}
