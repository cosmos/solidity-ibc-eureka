use std::time::Duration;

use l2_adapter::{
    adapters::optimism::{OpConsensusClient, OpConsensusClientConfig},
    l2_adapter_client::L2Adapter,
};
use tokio::time::interval;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::fmt().init();

    let mut finalized_ticker = interval(Duration::from_secs(5));
    let mut unfinalized_ticker = interval(Duration::from_secs(3));

    let conf = OpConsensusClientConfig {
        url: "http://127.0.0.1:49461".into(),
    };
    let client = OpConsensusClient::from_config(&conf);

    loop {
        tokio::select! {
                    _ = finalized_ticker.tick() => {
                        let finalized = client.get_latest_finalized_block().await.unwrap();
                        tracing::info!("Finalized state: {:#?}", finalized);
                    }
                    _ = unfinalized_ticker.tick() => {
                        let unfinalized = client.get_latest_unfinalized_block().await.unwrap();
                        tracing::info!("Unfinalized state: {:#?}", unfinalized);
                    }
        }
    }
}
