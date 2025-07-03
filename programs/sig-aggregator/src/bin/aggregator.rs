use anyhow::Result;
use clap::Parser;
use sig_aggregator::{
    aggregator::AggregatorService,
    config::Config,
    rpc::{
        aggregator_server::AggregatorServer, AggregateRequest, aggregator_client::AggregatorClient,
    },
    cli::{Cli, Commands},
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
        Commands::Aggregator { config } => {
            let config = Config::load(config)?;
            let listen_addr = config.listen_addr;
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
            let aggregate_response = response.into_inner();
            println!(
                "Received aggregated attestation:\n  Height: {}\n  State: 0x{}\n Signature: {:?}",
                aggregate_response.height,
                hex::encode(aggregate_response.state),
                aggregate_response.sig_pubkey_pairs
            );
        }
    }

    Ok(())
}
