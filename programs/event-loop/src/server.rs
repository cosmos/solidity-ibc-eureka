use std::time::Duration;

use crate::{
    event::{AttestorData, Event, MonitoringData},
    workflow::{DummyAttestor, DummyMonitorer},
};

use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

const CONCURRENCY_LIMIT: usize = 24;

pub struct Server;

impl Server {
    pub fn new() -> Self {
        Server {}
    }

    pub async fn start(
        &self,
        att: impl DummyAttestor,
        mon: impl DummyMonitorer,
        rx_monitoring_traffic: mpsc::Receiver<String>,
        rx_attestor_traffic: mpsc::Receiver<String>,
    ) -> Result<(), anyhow::Error> {
        let (tx, rx) = mpsc::channel::<Event>(1_000);

        let (tx_mon_clone, tx_att_clone) = (tx.clone(), tx.clone());
        tokio::spawn(async move {
            start_monitoring_service(mon, tx_mon_clone.clone(), rx_monitoring_traffic).await
        });
        tokio::spawn(async move {
            start_att_service(att, tx_att_clone.clone(), rx_attestor_traffic).await
        });

        let stream = ReceiverStream::new(rx);
        stream
            .map(async |event| match event {
                Event::Monitoring(_) => {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    tracing::info!("monitoring event occured")
                }
                Event::Attestor(_) => {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    tracing::info!("attestor event occured")
                }
            })
            .buffer_unordered(CONCURRENCY_LIMIT)
            .for_each(|_| async {
                tracing::info!("postprocess event");
            })
            .await;

        Ok(())
    }
}

async fn start_monitoring_service(
    mon: impl DummyMonitorer,
    tx: mpsc::Sender<Event>,
    mut rx_mon: mpsc::Receiver<String>,
) -> Result<(), anyhow::Error> {
    tracing::info!("monitoring service started");
    loop {
        if let Some(_) = rx_mon.recv().await {
            let _ = mon.get_monitoring_results().await;
            let _ = tx.send(Event::Monitoring(MonitoringData)).await;
        }
    }
}

async fn start_att_service(
    att: impl DummyAttestor,
    tx: mpsc::Sender<Event>,
    mut rx_att: mpsc::Receiver<String>,
) -> Result<(), anyhow::Error> {
    tracing::info!("attender service started");
    loop {
        if let Some(_) = rx_att.recv().await {
            let _ = att.get_l2_data().await;
            let _ = tx.send(Event::Attestor(AttestorData)).await;
        }
    }
}
