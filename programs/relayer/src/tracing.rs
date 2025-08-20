//! Tracing configuration for the relayer.
//!
//! Stdout logging + optional OpenTelemetry OTLP export.

use anyhow::{Context, Result};
use opentelemetry::{trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    resource::Resource,
    trace::{Sampler, SdkTracerProvider, SpanExporter, Tracer},
};
use std::env;
use tracing::Level;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

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

/// Guard that shuts down OpenTelemetry on drop (if enabled).
pub struct TracingGuard {
    otel_provider: Option<SdkTracerProvider>,
}

impl Drop for TracingGuard {
    fn drop(&mut self) {
        if let Some(provider) = self.otel_provider.take() {
            let _ = provider.shutdown();
        }
    }
}

/// Initialize the global tracing subscriber.
///
/// Keep the returned guard alive for the program’s lifetime to ensure a
/// clean OpenTelemetry shutdown.
pub fn init_subscriber(config: TracingConfig) -> Result<TracingGuard> {
    if env::var_os(RUST_LOG).is_none() {
        env::set_var(RUST_LOG, config.level.as_str().to_lowercase());
    }

    let otel_provider = if config.use_otel {
        match setup_otlp_tracer(&config) {
            Ok((tracer, provider)) => {
                let layer = tracing_opentelemetry::layer().with_tracer(tracer);

                let subscriber =
                    Registry::default()
                        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                            EnvFilter::new(config.level.as_str().to_lowercase())
                        }))
                        .with(
                            fmt::layer()
                                .pretty()
                                .with_target(true)
                                .with_line_number(true)
                                .with_file(config.with_file),
                        )
                        .with(layer);

                subscriber
                    .try_init()
                    .context("Failed to set global default subscriber")?;

                Some(provider)
            }
            Err(e) => {
                eprintln!("OpenTelemetry disabled: {e}");
                None
            }
        }
    } else {
        // Create a fmt subscriber without OpenTelemetry
        let subscriber = Registry::default()
            .with(
                EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| EnvFilter::new(config.level.as_str().to_lowercase())),
            )
            .with(
                fmt::layer()
                    .pretty()
                    .with_target(true)
                    .with_line_number(true)
                    .with_file(config.with_file),
            );

        subscriber
            .try_init()
            .context("Failed to set global default subscriber")?;

        None
    };

    Ok(TracingGuard { otel_provider })
}

/// Build an OTLP tracer + gRPC provider
fn setup_otlp_tracer(config: &TracingConfig) -> Result<(Tracer, SdkTracerProvider)> {
    let resource = Resource::builder()
        .with_attributes(vec![
            KeyValue::new("service.name", config.service_name.clone()),
            KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
        ])
        .build();

    let exporter = build_otlp_grpc_exporter(config)?;

    let provider = SdkTracerProvider::builder()
        .with_resource(resource)
        .with_sampler(Sampler::AlwaysOn)
        .with_batch_exporter(exporter)
        .build();

    // Use the trait method `.tracer(...)`
    let tracer = provider.tracer(config.service_name.clone());

    Ok((tracer, provider))
}

fn build_otlp_grpc_exporter(config: &TracingConfig) -> Result<impl SpanExporter> {
    let mut exporter_builder = opentelemetry_otlp::SpanExporter::builder().with_tonic();

    if let Some(endpoint) = &config.otel_endpoint {
        exporter_builder = exporter_builder.with_endpoint(endpoint);
    }

    Ok(exporter_builder.build()?)
}
