use clap::Parser;
use ibc_attestor::{
    adapter_client::Adapter,
    attestation_store::AttestationStore,
    attestor::AttestorService,
    cli::{AttestorCli, AttestorConfig, Commands},
    server::Server,
    signer::Signer,
    SolanaClient,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = AttestorCli::parse();

    match cli.command {
        Commands::Start(args) => {
            let config = AttestorConfig::from_file(args.config)?;

            tracing_subscriber::fmt::fmt()
                .with_max_level(config.server.log_level())
                .init();
            let signer = Signer::from_config(config.signer);

            #[cfg(feature = "sol")]
            {
                let adapter = SolanaClient::from_config(config.solana);
                let att_store = AttestationStore::new(adapter.block_time_ms());
                let attestor = AttestorService::new(adapter, signer, att_store);

                let server = Server::new();
                let _ = server.start(attestor, config.server).await;
            }

            Ok(())
        }
    }
}
