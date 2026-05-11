use std::path::PathBuf;

use clap::Parser;
use ibc_eureka_proof_api::cli::{Commands, ProofApiCli};
use ibc_eureka_proof_api::observability::init_observability;
use ibc_eureka_proof_api_core::{builder::ProofApiBuilder, config::ProofApiConfig};
use ibc_eureka_proof_api_cosmos_to_cosmos::CosmosToCosmosProofApiModule;
use ibc_eureka_proof_api_cosmos_to_eth::CosmosToEthProofApiModule;
use ibc_eureka_proof_api_cosmos_to_solana::CosmosToSolanaProofApiModule;
use ibc_eureka_proof_api_eth_to_cosmos::EthToCosmosProofApiModule;
use ibc_eureka_proof_api_eth_to_eth::EthToEthProofApiModule;
use ibc_eureka_proof_api_eth_to_solana::EthToSolanaProofApiModule;
use ibc_eureka_proof_api_solana_to_cosmos::SolanaToCosmosProofApiModule;
use ibc_eureka_proof_api_solana_to_eth::SolanaToEthProofApiModule;

use prometheus::{Encoder, TextEncoder};
use tracing::info;
use warp::Filter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = ProofApiCli::parse();
    match cli.command {
        Commands::Start(args) => {
            let config_path = PathBuf::from(args.config);
            let config_bz = std::fs::read(config_path)?;
            let config: ProofApiConfig = serde_json::from_slice(&config_bz)?;

            let _guard = init_observability(&config.observability)?;

            info!(
                "Observability initialized with level: {}",
                config.observability.level()
            );

            // Build the proof API server.
            let mut proof_api_builder = ProofApiBuilder::default();
            proof_api_builder.add_module(CosmosToEthProofApiModule);
            proof_api_builder.add_module(CosmosToCosmosProofApiModule);
            proof_api_builder.add_module(EthToCosmosProofApiModule);
            proof_api_builder.add_module(EthToEthProofApiModule);
            proof_api_builder.add_module(SolanaToCosmosProofApiModule);
            proof_api_builder.add_module(CosmosToSolanaProofApiModule);
            proof_api_builder.add_module(EthToSolanaProofApiModule);
            proof_api_builder.add_module(SolanaToEthProofApiModule);

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

            // Start the proof API server.
            proof_api_builder.start(config).await?;

            Ok(())
        }
    }
}
