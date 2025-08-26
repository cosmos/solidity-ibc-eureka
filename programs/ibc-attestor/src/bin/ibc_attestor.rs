use std::fs;

use clap::Parser;
use ibc_attestor::cli::{
    key::KeyCommands, AttestorCli, AttestorConfig, Commands, IBC_ATTESTOR_DIR, IBC_ATTESTOR_PATH,
};
use key_utils::pem::{generate_private_key_pem, read_private_key_pem};

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
                    #[allow(clippy::borrow_interior_mutable_const)]
                    let attestor_path = &*IBC_ATTESTOR_PATH;
                    if attestor_dir.exists() && attestor_path.exists() {
                        return Err(anyhow::anyhow!("key pair already found; aborting"));
                    }
                    generate_private_key_pem(attestor_path)
                        .map_err(|e| anyhow::anyhow!("unable to generate key {e}"))?;
                    println!(
                        "key successfully saved to {}",
                        attestor_path.to_str().unwrap()
                    );
                    Ok::<(), anyhow::Error>(())
                }
                KeyCommands::Show(args) => {
                    let mut printed_any = false;

                    if args.show_private {
                        #[allow(clippy::borrow_interior_mutable_const)]
                        let attestor_path = &*IBC_ATTESTOR_PATH;
                        let skey = std::fs::read_to_string(attestor_path).map_err(|_| {
                            anyhow::anyhow!(
                                "no key found at {}, please run `key generate`",
                                attestor_path.to_str().unwrap()
                            )
                        })?;
                        let skey = skey.trim_end_matches('\n');
                        print!("{skey}");
                        printed_any = true;
                    }

                    // Separate by newline
                    if printed_any {
                        println!("\n");
                    }

                    if args.show_public {
                        #[allow(clippy::borrow_interior_mutable_const)]
                        let attestor_path = &*IBC_ATTESTOR_PATH;
                        let signer = read_private_key_pem(attestor_path).map_err(|_| {
                            anyhow::anyhow!(
                                "no key found at {}, please run `ibc_attestor key generate`",
                                attestor_path.to_str().unwrap()
                            )
                        })?;
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
