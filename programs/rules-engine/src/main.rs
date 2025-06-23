use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration as StdDuration;

#[derive(Debug)]
enum MonitoringEvent {
    LastBatchTimestamp(DateTime<Utc>),
    TxInclusionRate { address: String, rate: f64 },
    TxReordered(String),
}

struct RuleEngine;

impl RuleEngine {
    fn handle_event(event: MonitoringEvent) {
        match event {
            MonitoringEvent::LastBatchTimestamp(ts) => {
                let delay = (Utc::now() - ts).num_seconds();
                if delay > 120 {
                    println!("[!] Send Msg to Relayer: Sequencer delay detected: {}s since last batch.", delay);
                }
            }
            MonitoringEvent::TxInclusionRate { address, rate } => {
                if rate < 0.5 {
                    println!("[!] Send Msg to Relayer: Possible censorship: {} inclusion rate = {:.2}", address, rate);
                }
            }
            MonitoringEvent::TxReordered(tx_hash) => {
                println!("[!] Send Msg to Relayer: Front-running suspected for tx: {}", tx_hash);
            }
        }
    }

    fn run(receiver: Receiver<MonitoringEvent>) {
        for event in receiver.iter() {
            RuleEngine::handle_event(event);
        }
    }
}

fn start_mock_data_feeder(sender: mpsc::Sender<MonitoringEvent>) {
    thread::spawn(move || loop {
        let mut rng = rand::rng();

        // Send a mock batch timestamp event
        let seconds_ago = rng.random_range(60..=240);
        let batch_time = Utc::now() - Duration::seconds(seconds_ago);
        sender.send(MonitoringEvent::LastBatchTimestamp(batch_time)).unwrap();

        // Send a mock inclusion rate event
        let addresses = ["0xabc...123", "0xdef...456"];
        for &addr in &addresses {
            let rate = rng.random_range(0.3..=1.0);
            sender
                .send(MonitoringEvent::TxInclusionRate {
                    address: addr.to_string(),
                    rate,
                })
                .unwrap();
        }

        // Send a mock reordering event occasionally
        if rng.random_bool(0.25) {
            sender.send(MonitoringEvent::TxReordered("0xtx1".to_string())).unwrap();
        }

        thread::sleep(StdDuration::from_secs(5));
    });
}

fn main() {
    let (tx, rx) = mpsc::channel();
    start_mock_data_feeder(tx);
    RuleEngine::run(rx);
}
