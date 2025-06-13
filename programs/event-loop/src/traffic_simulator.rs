use tokio::sync::mpsc;
use warp::Filter;

pub fn open_simulator_channels() -> (mpsc::Sender<String>, mpsc::Receiver<String>) {
    mpsc::channel(1_000)
}

pub async fn start_traffic_simulator(
    transmit_monitoring_traffic: mpsc::Sender<String>,
    transmit_attestor_traffic: mpsc::Sender<String>,
    port: u16,
) -> Result<(), anyhow::Error> {
    let send_mon = warp::any().map(move || transmit_monitoring_traffic.clone());
    let monitoring = warp::path("monitoring")
        .and(warp::get())
        .and(send_mon.clone())
        .and_then(async |tx: mpsc::Sender<String>| {
            let _ = tx.send("foo".into()).await;

            Ok::<_, warp::Rejection>(warp::reply::json(&serde_json::json!({
            "message": "monitoring event created",
            })))
        });

    let send_att = warp::any().map(move || transmit_attestor_traffic.clone());
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
    tracing::info!("traffic simulator listening on {address}");
    warp::serve(routes).run(([0, 0, 0, 0], port)).await;
    Ok(())
}
