use std::time::Duration;

use clap::Parser;
use ibc_attestor::{
    attestation_store::AttestationStore,
    attestor::AttestorService,
    cli::{server::ServerKind, AttestorCli, AttestorConfig, Commands},
    server::Server,
    signer::Signer,
    SolanaClient,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = AttestorCli::parse();

    match cli.command {
        Commands::Server(kind) => {
            match kind {
                ServerKind::Solana(args) => {
                    let config = AttestorConfig::from_file(args.config)?;

                    tracing_subscriber::fmt::fmt()
                        .with_max_level(config.server.log_level())
                        .init();
                    let signer = Signer::from_config(config.signer);

                    let adapter = SolanaClient::from_config(config.solana);
                    let att_store = AttestationStore::new(&adapter);
                    let solana_update_frequeny = Duration::from_millis(100);
                    let attestor =
                        AttestorService::new(adapter, signer, att_store, solana_update_frequeny);

                    let server = Server::new();
                    let _ = server.start(attestor, config.server).await;
                }
            }
            Ok(())
        }
    }
}
