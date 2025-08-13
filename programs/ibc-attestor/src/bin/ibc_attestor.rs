use std::fs;

use clap::Parser;
use ibc_attestor::cli::{
    key::KeyCommands, AttestorCli, AttestorConfig, Commands, IBC_ATTESTOR_DIR, IBC_ATTESTOR_PATH,
};
use key_utils::{generate_secret_key, read_private_pem_to_string, read_public_key_to_string};

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
                    Ok::<(), anyhow::Error>(())
                }
                KeyCommands::Show(args) => {
                    let mut printed_any = false;

                    if !args.hide_private {
                        let skey =
                            read_private_pem_to_string(&*IBC_ATTESTOR_PATH).map_err(|_| {
                                anyhow::anyhow!(
                                    "no key found at {}, please run `ibc_attestor key generate`",
                                    IBC_ATTESTOR_PATH.to_str().unwrap()
                                )
                            })?;
                        let skey = skey.trim_end_matches('\n');
                        print!("{}", skey);
                        printed_any = true;
                    }

                    // Separate by newline
                    if printed_any {
                        println!("\n");
                    }

                    if !args.hide_public {
                        let pkey =
                            read_public_key_to_string(&*IBC_ATTESTOR_PATH).map_err(|_| {
                                anyhow::anyhow!(
                                    "no key found at {}, please run `ibc_attestor key generate`",
                                    IBC_ATTESTOR_PATH.to_str().unwrap()
                                )
                            })?;
                        let pkey = pkey.trim_end_matches('\n');
                        print!("{}", pkey);
                    }

                    Ok::<(), anyhow::Error>(())
                }
            }?
        }
    }
    Ok(())
}
