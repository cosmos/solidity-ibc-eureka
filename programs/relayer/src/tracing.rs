//! Tracing configuration for the relayer.
//!
//! Stdout logging + optional OpenTelemetry OTLP export.

use anyhow::{Context, Result};
use ibc_eureka_relayer_core::config::TracingConfig;
use opentelemetry::{global, trace::TracerProvider as _, KeyValue};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    resource::Resource,
    trace::{Sampler, SdkTracerProvider, SpanExporter, Tracer},
};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};

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
/// Keep the returned guard alive for the program's lifetime to ensure a
/// clean OpenTelemetry shutdown.
#[allow(clippy::missing_errors_doc)]
pub fn init_subscriber(config: &TracingConfig) -> Result<TracingGuard> {
    // Set up global propagator for context propagation
    global::set_text_map_propagator(TraceContextPropagator::new());

    // Using a closure instead of a function allows Rust to infer the correct
    // return type for each usage, avoiding type system constraints that would
    // arise from specifying a concrete return type like `fmt::Layer<Registry>`.
    let create_fmt_layer = || {
        fmt::layer()
            .pretty()
            .with_target(true)
            .with_line_number(true)
            .with_file(true)
    };

    let otel_provider = if config.use_otel {
        match setup_otlp_tracer(config) {
            Ok((tracer, provider)) => {
                let subscriber = Registry::default()
                    .with(EnvFilter::new(config.level().as_str().to_lowercase()))
                    .with(create_fmt_layer())
                    .with(tracing_opentelemetry::layer().with_tracer(tracer));

                try_init_subscriber(subscriber)?;
                Some(provider)
            }
            Err(e) => {
                tracing::warn!("OpenTelemetry disabled: {e}");
                None
            }
        }
    } else {
        // Create a fmt subscriber without OpenTelemetry
        let subscriber = Registry::default()
            .with(EnvFilter::new(config.level().as_str().to_lowercase()))
            .with(create_fmt_layer());

        try_init_subscriber(subscriber)?;
        None
    };

    Ok(TracingGuard { otel_provider })
}

/// Initialize the subscriber and handle errors.
fn try_init_subscriber(subscriber: impl SubscriberInitExt) -> Result<()> {
    subscriber
        .try_init()
        .context("Failed to set global default subscriber")
}

/// Build an OTLP tracer and a gRPC provider
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
