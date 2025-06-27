use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use sig_aggregator::{
    aggregator::AggregatorService,
    attestor::run_attestor_server,
    config::Config,
    rpc::{aggregator_server::AggregatorServer, AggregateRequest, aggregator_client::AggregatorClient},
};

// use crate::{
//     aggregator::AggregatorService,
//     attestor::run_attestor_server,
//     config::Config,
// };


#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run an attestor instance
    Attestor {
        #[arg(long, default_value = "127.0.0.1:50051")]
        addr: String,
        #[arg(long)]
        fail: bool,
        #[arg(long, default_value_t = 0)]
        delay_ms: u64,
    },
    /// Run the aggregator service
    Aggregator {
        #[arg(long, default_value = "config.toml")]
        config: String,
    },
    /// Run a relayer client to query the aggregator
    Relayer {
        #[arg(long, default_value = "http://127.0.0.1:50060")]
        aggregator_addr: String,
        #[arg(long, default_value_t = 100)]
        min_height: u64,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let cli = Cli::parse();

    match cli.command {
        Commands::Attestor {
            addr,
            fail,
            delay_ms,
        } => {
            run_attestor_server(addr, fail, delay_ms).await?;
        }
        Commands::Aggregator { config } => {
            let config = Config::load(config)?;
            let listen_addr = config.listen_addr.parse()?;
            let aggregator_service = AggregatorService::from_config(config).await?;

            tracing::info!("Aggregator service listening on {}", listen_addr);
            tonic::transport::Server::builder()
                .add_service(AggregatorServer::new(aggregator_service))
                .serve(listen_addr)
                .await?;
        }
        Commands::Relayer {
            aggregator_addr,
            min_height,
        } => {
            tracing::info!("Relayer querying aggregator at {}", aggregator_addr);
            let mut client = AggregatorClient::connect(aggregator_addr).await?;

            let request = tonic::Request::new(AggregateRequest { min_height });

            let response = client.get_aggregate_attestation(request).await?;

            if let Some(attestation) = response.into_inner().attestation {
                println!(
                    "Received aggregated attestation:\n  Height: {}\n  Signature: 0x{}",
                    attestation.height,
                    hex::encode(attestation.signature)
                );
            } else {
                println!(
                    "Aggregator did not find an attestation with quorum for height >= {}",
                    min_height
                );
            }
        }
    }

    Ok(())
}
