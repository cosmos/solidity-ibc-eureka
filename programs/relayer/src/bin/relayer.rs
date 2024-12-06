use std::{path::PathBuf, sync::Arc};

use clap::Parser;
use solidity_ibc_eureka_relayer::{
    cli::{
        cmd::{Commands, RelayerCli},
        config::RelayerConfig,
    },
    core::builder::RelayerBuilder,
    modules::cosmos_to_eth::CosmosToEthRelayerModule,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = RelayerCli::parse();
    match cli.command {
        Commands::Start(args) => {
            let config_path = PathBuf::from(args.config);
            let config_bz = std::fs::read(config_path)?;
            let config: RelayerConfig = serde_json::from_slice(&config_bz)?;

            // Initialize the logger with log level.
            tracing_subscriber::fmt::fmt()
                .with_max_level(config.server.log_level())
                .init();

            // Build the relayer server.
            let mut relayer_builder = RelayerBuilder::default();
            relayer_builder.add_module(Arc::new(CosmosToEthRelayerModule));

            // Start the relayer server.
            relayer_builder.start_server(config).await?;

            Ok(())
        }
    }
}
