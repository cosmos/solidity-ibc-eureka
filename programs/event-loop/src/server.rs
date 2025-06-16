use std::time::Duration;

use crate::workflow::{DummyAttestor, DummyMonitorer};

pub struct Server;

impl Server {
    pub fn new() -> Self {
        Server {}
    }

    pub async fn start(
        &self,
        att: impl DummyAttestor,
        mon: impl DummyMonitorer,
    ) -> Result<(), anyhow::Error> {
        let mut mon_ticker = tokio::time::interval(Duration::from_secs(5));
        let mut att_ticker = tokio::time::interval(Duration::from_secs(3));

        loop {
            tokio::select! {
                _ = mon_ticker.tick() => {
                    let _ = mon.get_monitoring_results().await;
                    tracing::info!("monitoring event occurred");
                }
                _ = att_ticker.tick() => {
                    let _ = att.get_l2_data().await;
                    tracing::info!("attestor event occurred");
                }
            }
        }
    }
}
