use std::{env, fs, path::PathBuf};

use clap::Parser;
use ibc_attestor::{
    adapter_client::Adapter,
    attestation_store::AttestationStore,
    attestor::AttestorService,
    cli::{
        key::{self, KeyCommands},
        server::ServerKind,
        AttestorCli, AttestorConfig, Commands,
    },
    server::Server,
    signer::Signer,
    SolanaClient,
};
use key_utils::{generate_secret_key, read_private_pem_to_string, read_public_key_to_string};

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
                    let signer = Signer::from_config(config.signer.unwrap_or_default());

                    let adapter = SolanaClient::from_config(config.solana);
                    let att_store = AttestationStore::new(adapter.block_time_ms());
                    let attestor = AttestorService::new(adapter, signer, att_store);

                    let server = Server::new();
                    let _ = server.start(attestor, config.server).await;
                }
            }
            Ok(())
        }
        Commands::Key(cmd) => {
            let home = env::var("HOME").map(PathBuf::from).unwrap();
            let attestor_dir = home.join(".ibc-attestor");
            let key_home = attestor_dir.join("ibc-attestor.pem");

            if !attestor_dir.exists() {
                fs::create_dir_all(&attestor_dir).unwrap();
            }

            match cmd {
                KeyCommands::Generate => {
                    if attestor_dir.exists() && key_home.exists() {
                        return Err(anyhow::anyhow!("key pair already found; aborting"));
                    }
                    generate_secret_key(&key_home)
                        .map_err(|e| anyhow::anyhow!("unable to generate key {e}"))?;
                    println!("key successfully saved to {}", key_home.to_str().unwrap());
                    Ok(())
                }
                KeyCommands::Show => {
                    let skey = read_private_pem_to_string(&key_home).map_err(|_| {
                        anyhow::anyhow!(
                            "no key found at {}, please run `ibc_attestor key generate`",
                            key_home.to_str().unwrap()
                        )
                    })?;
                    println!("secret key:\n{}", skey);

                    let pkey = read_public_key_to_string(&key_home).map_err(|_| {
                        anyhow::anyhow!(
                            "no key found at {}, please run `ibc_attestor key generate`",
                            key_home.to_str().unwrap()
                        )
                    })?;
                    println!("public key as hex string:\n{}", pkey);

                    Ok(())
                }
            }
        }
    }
}
