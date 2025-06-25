use std::time::Duration;

use l2_adapter::{
    adapters::{
        arbitrum::{ArbitrumClient, ArbitrumClientConfig},
        optimism::{OpConsensusClient, OpConsensusClientConfig},
    },
    l2_adapter_client::L2Adapter,
};
use tokio::time::interval;

const OP_URL: &str = "http://127.0.0.1:49461";
const ARB_URL: &str = "http://127.0.0.1:8547";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::fmt().init();

    let mut op_finalized_ticker = interval(Duration::from_secs(5));
    let mut op_unfinalized_ticker = interval(Duration::from_secs(3));

    let op_conf = OpConsensusClientConfig { url: OP_URL.into() };
    let op_client = OpConsensusClient::from_config(&op_conf);

    let mut arb_finalized_ticker = interval(Duration::from_secs(5));
    let mut arb_unfinalized_ticker = interval(Duration::from_secs(3));

    let arb_conf = ArbitrumClientConfig {
        url: ARB_URL.into(),
    };
    let arb_client = ArbitrumClient::from_config(&arb_conf);

    loop {
        tokio::select! {
                    _ = op_finalized_ticker.tick() => {
                        let finalized = op_client.get_latest_finalized_block().await.unwrap();
                        tracing::info!("Op Stack finalized state: {:#?}", finalized);
                    }
                    _ = op_unfinalized_ticker.tick() => {
                        let unfinalized = op_client.get_latest_unfinalized_block().await.unwrap();
                        tracing::info!("Op stack unfinalized state: {:#?}", unfinalized);
                    }

                    _ = arb_finalized_ticker.tick() => {
                        let finalized = arb_client.get_latest_finalized_block().await.unwrap();
                        tracing::info!("Arbitrum finalized state: {:#?}", finalized);
                    }
                    _ = arb_unfinalized_ticker.tick() => {
                        let unfinalized = arb_client.get_latest_unfinalized_block().await.unwrap();
                        tracing::info!("Arbitrum unfinalized state: {:#?}", unfinalized);
                    }
        }
    }
}
