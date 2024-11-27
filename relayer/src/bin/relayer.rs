use std::path::PathBuf;

use clap::Parser;
use solidity_ibc_eureka_relayer::{
    cli::{
        cmd::{Commands, RelayerCli},
        config::RelayerConfig,
    },
    core::{builder::RelayerBuilder, modules::RelayerModule},
    modules::cosmos_to_eth::{CosmosToEthConfig, CosmosToEthRelayerModule},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = RelayerCli::parse();
    match cli.command {
        Commands::Start(args) => {
            let config_path = PathBuf::from(args.config);
            let config_bz = std::fs::read(config_path)?;
            let config: RelayerConfig = serde_json::from_slice(&config_bz)?;

            // Initialize a Cosmos to Ethereum relayer module.
            let cosmos_to_eth_config_value = config
                .modules
                .iter()
                .find(|module| module.name == CosmosToEthRelayerModule::NAME)
                .expect("Cosmos to Ethereum module not found")
                .config
                .clone();
            let cosmos_to_eth_config: CosmosToEthConfig =
                serde_json::from_value(cosmos_to_eth_config_value)?;

            let cosmos_to_eth_module = CosmosToEthRelayerModule::new(cosmos_to_eth_config).await;

            // Build the relayer server.
            let mut relayer_builder = RelayerBuilder::default();
            relayer_builder.set_address(&config.server.address);
            relayer_builder.set_starting_port(config.server.starting_port);
            relayer_builder.add_module(
                CosmosToEthRelayerModule::NAME,
                Box::new(cosmos_to_eth_module),
            );

            todo!()
        }
    }
}
