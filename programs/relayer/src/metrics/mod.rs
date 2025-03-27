//! Metrics module for the relayer.

#![allow(missing_docs)]
use lazy_static::lazy_static;
use prometheus::{
    register_counter, register_histogram_vec, register_int_counter_vec, register_int_gauge,
    Counter, HistogramVec, IntCounterVec, IntGauge,
};

lazy_static! {
    pub static ref REQUEST_COUNTER: Counter =
        register_counter!("request_total", "Total number of requests").unwrap();
    pub static ref RESPONSE_TIME: HistogramVec = register_histogram_vec!(
        "response_time_seconds",
        "Response time in seconds",
        &["method", "path"]
    )
    .unwrap();
    pub static ref RESPONSE_CODE: IntCounterVec = register_int_counter_vec!(
        "response_code",
        "Response Codes",
        &["method", "path", "status_code"]
    )
    .unwrap();
    pub static ref CONNECTED_CLIENTS: IntGauge =
        register_int_gauge!("connected_clients", "Connected clients").unwrap();
}
