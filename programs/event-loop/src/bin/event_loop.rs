use std::path::PathBuf;

use clap::Parser;
use event_loop::{
    cli::{
        cmd::{AttestorCli, Commands},
        config::AttestorConfig,
    },
    server::Server,
    traffic_simulator::{open_simulator_channels, start_traffic_simulator},
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

            let monitoring_simulator_channel = open_simulator_channels();
            let attestor_simulator_channel = open_simulator_channels();
            tokio::spawn(async move {
                start_traffic_simulator(
                    monitoring_simulator_channel.0,
                    attestor_simulator_channel.0,
                    config.server.port,
                )
                .await
            });
            let server = Server::new();
            server
                .start(
                    Att,
                    Mon,
                    monitoring_simulator_channel.1,
                    attestor_simulator_channel.1,
                )
                .await?;

            Ok(())
        }
    }
}
