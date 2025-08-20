//! Tracing configuration for the relayer.
//!
//! This module provides functions to configure and initialize tracing for the relayer.
//! It supports both standard output logging and OpenTelemetry integration for distributed tracing.

use anyhow::Result;
use opentelemetry::trace::TracerProvider as _; // trait for .versioned_tracer(...)
use opentelemetry::KeyValue;
use opentelemetry_otlp::exporter::trace::TonicExporterBuilder;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    resource::Resource,
    trace::{Sampler, SdkTracerProvider, Tracer},
};
use std::env;
use tracing::Level;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{
    fmt::{self},
    layer::SubscriberExt,
    EnvFilter, Registry,
};

/// Default environment variable for controlling the log level.
const RUST_LOG: &str = "RUST_LOG";

/// Default service name for OpenTelemetry.
const DEFAULT_SERVICE_NAME: &str = "ibc-eureka-relayer";

/// Configuration for the tracing subscriber.
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// The log level to use.
    pub level: Level,
    /// Whether to include file paths in logs.
    pub with_file: bool,
    /// Whether to use OpenTelemetry for distributed tracing.
    pub use_otel: bool,
    /// The service name to use for OpenTelemetry.
    pub service_name: String,
    /// The OpenTelemetry collector endpoint.
    pub otel_endpoint: Option<String>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            with_file: true,
            use_otel: false,
            service_name: DEFAULT_SERVICE_NAME.to_string(),
            otel_endpoint: None,
        }
    }
}

/// Initialize the tracing subscriber based on the provided configuration.
///
/// This function sets up a tracing subscriber with the provided configuration.
/// It returns a guard that should be kept alive for the duration of the program.
///
/// # Examples
///
/// ```
/// use ibc_eureka_relayer::tracing::{init_subscriber, TracingConfig};
/// use tracing::Level;
///
/// let config = TracingConfig {
///    level: Level::DEBUG,
///    ..Default::default()
/// };
///
/// let _guard = init_subscriber(config);
/// ```
pub fn init_subscriber(config: TracingConfig) -> Result<TracingGuard> {
    // Set the default log level if RUST_LOG is not set
    if env::var(RUST_LOG).is_err() {
        env::set_var(RUST_LOG, config.level.as_str().to_lowercase());
    }

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.level.as_str().to_lowercase()));

    // Configure the formatter layer
    let fmt_layer = fmt::layer()
        .pretty()
        .with_target(true)
        .with_line_number(true)
        .with_file(config.with_file);

    let (subscriber, otel_provider) = if config.use_otel {
        match setup_opentelemetry_layer(&config) {
            Ok((otel_layer, provider)) => {
                let sub = Registry::default()
                    .with(env_filter)
                    .with(fmt_layer)
                    .with(otel_layer);
                (sub, Some(provider))
            }
            Err(e) => {
                eprintln!("Failed to initialize OpenTelemetry: {e}");
                let sub = Registry::default().with(env_filter).with(fmt_layer);
                (sub, None)
            }
        }
    } else {
        let sub = Registry::default().with(env_filter).with(fmt_layer);
        (sub, None)
    };

    subscriber
        .try_init()
        .context("Failed to set global default subscriber")?;

    // Return a guard that keeps the OpenTelemetry provider alive
    Ok(TracingGuard { otel_provider })
}

/// Guard that shuts down OpenTelemetry on drop (if enabled).
pub struct TracingGuard {
    otel_provider: Option<SdkTracerProvider>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.otel_provider.take() {
            // Best-effort shutdown/flush
            let _ = provider.shutdown();
        }
    }
}

/// Build the OpenTelemetry layer (OTLP exporter) using opentelemetry 0.30 APIs.
fn setup_opentelemetry_layer(
    config: &TracingConfig,
) -> anyhow::Result<(OpenTelemetryLayer<Registry, Tracer>, SdkTracerProvider)> {
    let resource = Resource::builder()
        .with_attributes(vec![
            KeyValue::new("service.name", config.service_name.clone()),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ])
        .build();

    // gRPC exporter (requires feature `grpc-tonic`)
    let mut builder = TonicExporterBuilder::default();
    if let Some(endpoint) = &config.otel_endpoint {
        builder = builder.with_endpoint(endpoint.clone()); // e.g. "http://localhost:4317"
    }
    let exporter = builder.build()?; // -> impl SpanExporter

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_sampler(Sampler::AlwaysOn)
        .with_batch_exporter(exporter)
        .build();

    let tracer =
        provider.versioned_tracer(&config.service_name, Some(env!("CARGO_PKG_VERSION")), None);

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    Ok((layer, provider))
}
