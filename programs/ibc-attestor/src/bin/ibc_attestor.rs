use std::fs;

use alloy_signer_local::PrivateKeySigner;
use clap::Parser;
use ethereum_keys::signer_local::{read_from_keystore, write_to_keystore};
use ibc_attestor::cli::{
    key::KeyCommands, AttestorCli, AttestorConfig, Commands, DEFAULT_KEYSTORE_NAME,
    IBC_ATTESTOR_DIR,
};

// Compile-time check: ensure that exactly one blockchain feature is enabled
#[cfg(not(any(feature = "sol", feature = "op", feature = "arbitrum")))]
compile_error!(
    "Please enable exactly one blockchain feature using --features sol, op, or arbitrum"
);

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = AttestorCli::parse();

    match cli.command {
        Commands::Server(args) => {
            let config = AttestorConfig::from_file(args.config)?;

            #[cfg(feature = "sol")]
            {
                ibc_attestor::server::run_solana_server(config.clone()).await?
            }
            #[cfg(feature = "op")]
            {
                ibc_attestor::server::run_optimism_server(config.clone()).await?
            }
            #[cfg(feature = "arbitrum")]
            {
                ibc_attestor::server::run_arbitrum_server(config.clone()).await?
            }
        }
        Commands::Key(cmd) => {
            #[allow(clippy::borrow_interior_mutable_const)]
            let attestor_dir = &*IBC_ATTESTOR_DIR;
            if !attestor_dir.exists() {
                fs::create_dir_all(attestor_dir).unwrap();
            }

            match cmd {
                KeyCommands::Generate => {
                    #[allow(clippy::borrow_interior_mutable_const)]
                    let attestor_dir = &*IBC_ATTESTOR_DIR;
                    let keystore_path = attestor_dir.join(DEFAULT_KEYSTORE_NAME);
                    if attestor_dir.exists() && keystore_path.exists() {
                        return Err(anyhow::anyhow!("key pair already found; aborting"));
                    }

                    let signer = PrivateKeySigner::random();
                    write_to_keystore(attestor_dir, DEFAULT_KEYSTORE_NAME, signer)
                        .map_err(|e| anyhow::anyhow!("unable to generate key {e}"))?;
                    println!(
                        "key successfully saved to {}",
                        keystore_path.to_str().unwrap()
                    );
                    Ok::<(), anyhow::Error>(())
                }
                KeyCommands::Show(args) => {
                    let mut printed_any = false;

                    let keystore_path = attestor_dir.join(DEFAULT_KEYSTORE_NAME);
                    if args.show_private {
                        let signer = read_from_keystore(keystore_path.clone())?;
                        print!("{}", hex::encode(signer.credential().to_bytes().as_slice()));
                        printed_any = true;
                    }

                    // Separate by newline
                    if printed_any {
                        println!("\n");
                    }

                    if args.show_public {
                        let signer = read_from_keystore(keystore_path)?;
                        let addr = signer.address();
                        print!("{}", hex::encode(addr.as_slice()));
                    }

                    Ok::<(), anyhow::Error>(())
                }
            }?
        }
    }
    Ok(())
}
