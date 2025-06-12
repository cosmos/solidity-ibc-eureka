use crate::{
    cli::config::ServerConfig,
    event::{AttestorData, Event, MonitoringData},
    workflow::{DummyAttestor, DummyMonitorer},
};

use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use warp::Filter;

const CONCURRENCY_LIMIT: usize = 8;

pub struct Server {
    port: u16,
}

impl Server {
    pub fn new(config: ServerConfig) -> Self {
        Server { port: config.port }
    }

    pub async fn start(
        &self,
        att: impl DummyAttestor,
        mon: impl DummyMonitorer,
    ) -> Result<(), anyhow::Error> {
        let (tx_mon, rx_mon) = mpsc::channel::<String>(16);
        let (tx_att, rx_att) = mpsc::channel::<String>(16);
        let (tx, rx) = mpsc::channel::<Event>(16);

        let (tx_mon_clone, tx_att_clone) = (tx.clone(), tx.clone());
        let port = self.port;
        tokio::spawn(async move { start_dev_server(tx_mon, tx_att, port).await });
        tokio::spawn(
            async move { start_monitoring_service(mon, tx_mon_clone.clone(), rx_mon).await },
        );
        tokio::spawn(async move { start_att_service(att, tx_att_clone.clone(), rx_att).await });

        let stream = ReceiverStream::new(rx);
        stream
            .map(async |event| match event {
                Event::Monitoring(_) => tracing::info!("monitoring event occured"),
                Event::Attestor(_) => tracing::info!("attestor event occured"),
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

async fn start_dev_server(
    tx_mon: mpsc::Sender<String>,
    tx_att: mpsc::Sender<String>,
    port: u16,
) -> Result<(), anyhow::Error> {
    let send_mon = warp::any().map(move || tx_mon.clone());
    let monitoring = warp::path("monitoring")
        .and(warp::get())
        .and(send_mon.clone())
        .and_then(async |tx: mpsc::Sender<String>| {
            let _ = tx.send("foo".into()).await;

            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
            "message": "monitoring event created",
            })))
        });

    let send_att = warp::any().map(move || tx_att.clone());
    let l2 = warp::path("l2")
        .and(warp::get())
        .and(send_att.clone())
        .and_then(async |tx: mpsc::Sender<String>| {
            let _ = tx.send("bar".into()).await;

            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
                "layer": "2",
                "message": "L2 endpoint up and running",
            })))
        });
    let routes = monitoring.or(l2);

    let address = format!("0.0.0.0:{port}");
    tracing::info!("listening on {address}");
    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
    Ok(())
}
