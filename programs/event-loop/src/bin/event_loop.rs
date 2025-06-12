use std::path::PathBuf;

use clap::Parser;
use event_loop::{
    cli::{
        cmd::{AttestorCli, Commands},
        config::AttestorConfig,
    },
    server::Server,
    workflow::{Att, Mon},
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = AttestorCli::parse();

    match cli.command {
        Commands::Start(args) => {
            let config_path = PathBuf::from(args.config);
            let config_bz = std::fs::read(config_path)?;
            let config: AttestorConfig = serde_json::from_slice(&config_bz)?;

            tracing_subscriber::fmt::fmt()
                .with_max_level(config.server.log_level())
                .init();

            let server = Server::new(config.server);
            server.start(Att, Mon).await?;

            Ok(())
        }
    }
}
