use std::fs;

use clap::Parser;
use ibc_attestor::{
    attestor::AttestorService,
    cli::{
        key::KeyCommands, server::ServerKind, AttestorCli, AttestorConfig, Commands,
        IBC_ATTESTOR_DIR, IBC_ATTESTOR_PATH,
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
                #[cfg(feature = "sol")]
                ServerKind::Solana(args) => {
                    let config = AttestorConfig::from_file(args.config)?;

                    let signer = Signer::from_config(config.signer.unwrap_or_default())?;

                    let adapter = SolanaClient::from_config(config.solana);
                    let attestor = AttestorService::new(adapter, signer);

                    let server = Server::new(&config.server);
                    let _ = server.start(attestor, config.server).await;
                }
            }
            Ok(())
        }
        Commands::Key(cmd) => {
            if !IBC_ATTESTOR_DIR.exists() {
                fs::create_dir_all(&*IBC_ATTESTOR_DIR).unwrap();
            }

            match cmd {
                KeyCommands::Generate => {
                    if IBC_ATTESTOR_DIR.exists() && IBC_ATTESTOR_PATH.exists() {
                        return Err(anyhow::anyhow!("key pair already found; aborting"));
                    }
                    generate_secret_key(&*IBC_ATTESTOR_PATH)
                        .map_err(|e| anyhow::anyhow!("unable to generate key {e}"))?;
                    println!(
                        "key successfully saved to {}",
                        IBC_ATTESTOR_PATH.to_str().unwrap()
                    );
                    Ok(())
                }
                KeyCommands::Show => {
                    let skey = read_private_pem_to_string(&*IBC_ATTESTOR_PATH).map_err(|_| {
                        anyhow::anyhow!(
                            "no key found at {}, please run `ibc_attestor key generate`",
                            IBC_ATTESTOR_PATH.to_str().unwrap()
                        )
                    })?;
                    println!("secret key:\n{}", skey);

                    let pkey = read_public_key_to_string(&*IBC_ATTESTOR_PATH).map_err(|_| {
                        anyhow::anyhow!(
                            "no key found at {}, please run `ibc_attestor key generate`",
                            IBC_ATTESTOR_PATH.to_str().unwrap()
                        )
                    })?;
                    println!("public key as hex string:\n{}", pkey);

                    Ok(())
                }
            }
        }
    }
}
