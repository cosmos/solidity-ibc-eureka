use chrono::{DateTime, Utc};
use std::sync::mpsc::Receiver;

#[derive(Debug)]
pub enum MonitoringEvent {
    LastBatchTimestamp(DateTime<Utc>),
    TxInclusionRate { address: String, rate: f64 },
    TxReordered(String),
}

pub struct RuleEngine;

impl RuleEngine {
    pub fn handle_event(event: MonitoringEvent) {
        match event {
            MonitoringEvent::LastBatchTimestamp(ts) => {
                let delay = (Utc::now() - ts).num_seconds();
                if delay > 120 {
                    println!("[!] Sequencer delay detected: {}s since last batch.", delay);
                }
            }
            MonitoringEvent::TxInclusionRate { address, rate } => {
                if rate < 0.5 {
                    println!("[!] Possible censorship: {} inclusion rate = {:.2}", address, rate);
                }
            }
            MonitoringEvent::TxReordered(tx_hash) => {
                println!("[!] Front-running suspected for tx: {}", tx_hash);
            }
        }
    }

    pub fn run(receiver: Receiver<MonitoringEvent>) {
        for event in receiver.iter() {
            RuleEngine::handle_event(event);
        }
    }
}
