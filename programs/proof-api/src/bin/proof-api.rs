use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;
use proof_api::cli::{Commands, ProofApiCli};
use proof_api::observability::init_observability;
use proof_api_core::{builder::ProofApiBuilder, config::ProofApiConfig};
#[cfg(feature = "cosmos-to-cosmos")]
use proof_api_cosmos_to_cosmos::CosmosToCosmosProofApiModule;
#[cfg(feature = "cosmos-to-eth")]
use proof_api_cosmos_to_eth::CosmosToEthProofApiModule;
#[cfg(feature = "solana")]
use proof_api_cosmos_to_solana::CosmosToSolanaProofApiModule;
#[cfg(feature = "eth-to-cosmos")]
use proof_api_eth_to_cosmos::EthToCosmosProofApiModule;
#[cfg(feature = "eth-to-eth")]
use proof_api_eth_to_eth::EthToEthProofApiModule;
#[cfg(feature = "solana")]
use proof_api_eth_to_solana::EthToSolanaProofApiModule;
#[cfg(feature = "solana")]
use proof_api_solana_to_cosmos::SolanaToCosmosProofApiModule;
#[cfg(feature = "solana")]
use proof_api_solana_to_eth::SolanaToEthProofApiModule;

use prometheus::{Encoder, TextEncoder};
use tokio::net::TcpStream;
use tracing::{error, info};
use warp::{http::StatusCode, Filter};

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

            let grpc_addr = format!("{}:{}", config.server.address, config.server.port)
                .parse::<SocketAddr>()?;

            // Build the proof API server.
            let mut proof_api_builder = ProofApiBuilder::default();
            #[cfg(feature = "cosmos-to-eth")]
            proof_api_builder.add_module(CosmosToEthProofApiModule);
            #[cfg(feature = "cosmos-to-cosmos")]
            proof_api_builder.add_module(CosmosToCosmosProofApiModule);
            #[cfg(feature = "eth-to-cosmos")]
            proof_api_builder.add_module(EthToCosmosProofApiModule);
            #[cfg(feature = "eth-to-eth")]
            proof_api_builder.add_module(EthToEthProofApiModule);
            #[cfg(feature = "solana")]
            proof_api_builder.add_module(SolanaToCosmosProofApiModule);
            #[cfg(feature = "solana")]
            proof_api_builder.add_module(CosmosToSolanaProofApiModule);
            #[cfg(feature = "solana")]
            proof_api_builder.add_module(EthToSolanaProofApiModule);
            #[cfg(feature = "solana")]
            proof_api_builder.add_module(SolanaToEthProofApiModule);

            // Start the metrics server.
            tokio::spawn(async move {
                let healthz_route = warp::get()
                    .and(warp::path("healthz".to_string()))
                    .and(warp::path::end())
                    .map(move || grpc_addr)
                    .then(check_grpc);

                let metrics_route = warp::path("metrics".to_string()).map(|| {
                    let encoder = TextEncoder::new();
                    let metric_families = prometheus::gather();
                    let mut buffer = Vec::new();
                    encoder.encode(&metric_families, &mut buffer).unwrap();
                    String::from_utf8(buffer).unwrap()
                });

                let routes = healthz_route.or(metrics_route);

                info!("Health check available at http://0.0.0.0:9000/healthz");
                info!("Metrics available at http://0.0.0.0:9000/metrics");
                warp::serve(routes).run(([0, 0, 0, 0], 9000)).await;
            });

            // Start the proof API server.
            proof_api_builder.start(config).await?;

            Ok(())
        }
    }
}

async fn check_grpc(grpc_addr: SocketAddr) -> StatusCode {
    match TcpStream::connect(grpc_addr).await {
        Ok(_) => StatusCode::OK,
        Err(e) => {
            error!(%grpc_addr, error = %e, "health check failed: gRPC server not ready");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}
