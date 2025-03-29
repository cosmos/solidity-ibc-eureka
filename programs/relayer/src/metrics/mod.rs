//! Metrics module for the relayer.

#![allow(missing_docs)]

use lazy_static::lazy_static;
use prometheus::{
    register_counter, register_histogram_vec, register_int_counter_vec, register_int_gauge,
    Counter, HistogramVec, IntCounterVec, IntGauge,
};
use std::time::Instant;

// Prometheus metrics for the relayer
lazy_static! {
    // Total number of requests
    pub static ref REQUEST_COUNTER: Counter =
        register_counter!("eureka_relayer_request_total", "Total number of requests").unwrap();
    // Response time in seconds, distinguished by method, src_chain, and dst_chain
    pub static ref RESPONSE_TIME: HistogramVec = register_histogram_vec!(
        "eureka_relayer_response_time_seconds",
        "Response time in seconds",
        &["method", "src_chain", "dst_chain"]
    )
    .unwrap();
    // Response Codes, distinguished by method, src_chain, dst_chain, and status_code
    pub static ref RESPONSE_CODE: IntCounterVec = register_int_counter_vec!(
        "eureka_relayer_response_codes",
        "Response Codes",
        &["method", "src_chain", "dst_chain", "status_code"]
    )
    .unwrap();
    // Number of connected clients, or concurrent requests
    pub static ref CONNECTED_CLIENTS: IntGauge =
        register_int_gauge!("eureka_relayer_connected_clients", "Connected clients").unwrap();
}

/// Generic metrics tracking middleware for service calls
/// # Errors
/// Returns an error if the function itself returns an error.
pub async fn track_metrics<F, Fut, R>(
    method: &str,
    src_chain: &str,
    dst_chain: &str,
    f: F,
) -> Result<tonic::Response<R>, tonic::Status>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<tonic::Response<R>, tonic::Status>>,
{
    let timer = Instant::now();
    CONNECTED_CLIENTS.inc();
    REQUEST_COUNTER.inc();

    let result = f().await;
    let status_code: isize = match &result {
        Ok(_) => 0,
        Err(status) => status.code() as isize,
    };

    RESPONSE_TIME
        .with_label_values(&[method, src_chain, dst_chain])
        .observe(timer.elapsed().as_secs_f64());

    RESPONSE_CODE
        .with_label_values(&[method, src_chain, dst_chain, &status_code.to_string()])
        .inc();

    CONNECTED_CLIENTS.dec();
    result
}
