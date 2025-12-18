use std::path::PathBuf;

use clap::Parser;
use ibc_eureka_relayer::cli::{Commands, RelayerCli};
use ibc_eureka_relayer::observability::init_observability;
use ibc_eureka_relayer_core::{builder::RelayerBuilder, config::RelayerConfig};
use ibc_eureka_relayer_cosmos_to_cosmos::CosmosToCosmosRelayerModule;
use ibc_eureka_relayer_cosmos_to_eth::CosmosToEthRelayerModule;
use ibc_eureka_relayer_cosmos_to_solana::CosmosToSolanaRelayerModule;
use ibc_eureka_relayer_eth_to_cosmos::EthToCosmosRelayerModule;
use ibc_eureka_relayer_eth_to_cosmos_compat::EthToCosmosCompatRelayerModule;
use ibc_eureka_relayer_solana_to_cosmos::SolanaToCosmosRelayerModule;

use prometheus::{Encoder, TextEncoder};
use tracing::info;
use warp::Filter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = RelayerCli::parse();
    match cli.command {
        Commands::Start(args) => {
            let config_path = PathBuf::from(args.config);
            let config_bz = std::fs::read(config_path)?;
            let config: RelayerConfig = serde_json::from_slice(&config_bz)?;

            let _guard = init_observability(&config.observability)?;

            info!(
                "Observability initialized with level: {}",
                config.observability.level()
            );

            // Build the relayer server.
            let mut relayer_builder = RelayerBuilder::default();
            relayer_builder.add_module(CosmosToEthRelayerModule);
            relayer_builder.add_module(CosmosToCosmosRelayerModule);
            relayer_builder.add_module(EthToCosmosRelayerModule);
            relayer_builder.add_module(EthToCosmosCompatRelayerModule);
            relayer_builder.add_module(SolanaToCosmosRelayerModule);
            relayer_builder.add_module(CosmosToSolanaRelayerModule);

            // Start the metrics server.
            tokio::spawn(async {
                let metrics_route = warp::path("metrics".to_string()).map(|| {
                    let encoder = TextEncoder::new();
                    let metric_families = prometheus::gather();
                    let mut buffer = Vec::new();
                    encoder.encode(&metric_families, &mut buffer).unwrap();
                    String::from_utf8(buffer).unwrap()
                });

                info!("Metrics available at http://0.0.0.0:9000/metrics");
                warp::serve(metrics_route).run(([0, 0, 0, 0], 9000)).await;
            });

            // Start the relayer server.
            relayer_builder.start(config).await?;

            Ok(())
        }
    }
}
