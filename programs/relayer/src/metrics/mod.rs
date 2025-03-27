//! Metrics module for the relayer.

#![allow(missing_docs)]
use lazy_static::lazy_static;
use prometheus::{
    register_counter, register_histogram_vec, register_int_counter_vec, register_int_gauge,
    Counter, HistogramVec, IntCounterVec, IntGauge,
};
use std::time::Instant;

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

/// Generic metrics tracking middleware for service calls
/// # Errors
/// Returns an error if the function itself returns an error.
pub async fn track_metrics<F, Fut, T, R>(
    method: &str,
    service: &str,
    request: tonic::Request<T>,
    f: F,
) -> Result<tonic::Response<R>, tonic::Status>
where
    F: FnOnce(tonic::Request<T>) -> Fut,
    Fut: std::future::Future<Output = Result<tonic::Response<R>, tonic::Status>>,
{
    let timer = Instant::now();
    CONNECTED_CLIENTS.inc();
    REQUEST_COUNTER.inc();

    let result = f(request).await;

    let status_code = match &result {
        Ok(_) => "success".to_string(),
        Err(status) => status.code().to_string(),
    };

    RESPONSE_TIME
        .with_label_values(&[method, service])
        .observe(timer.elapsed().as_secs_f64());

    RESPONSE_CODE
        .with_label_values(&[method, service, &status_code])
        .inc();

    CONNECTED_CLIENTS.dec();
    result
}
