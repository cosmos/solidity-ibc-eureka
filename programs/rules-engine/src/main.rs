use chrono::{Duration, Utc};
use rand::Rng;
use std::sync::mpsc;
use std::thread;
use std::time::Duration as StdDuration;
use rules_engine::{MonitoringEvent, RuleEngine};


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
