//! OpenTelemetry context propagation utilities for gRPC.

use opentelemetry::trace::TraceContextExt;
use tonic::{metadata::MetadataMap, Request};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// OpenTelemetry injector for gRPC metadata.
pub struct MetadataInjector<'a>(pub &'a mut MetadataMap);

impl opentelemetry::propagation::Injector for MetadataInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        match tonic::metadata::MetadataKey::from_bytes(key.as_bytes()) {
            Ok(key) => match tonic::metadata::MetadataValue::try_from(&value) {
                Ok(value) => {
                    self.0.insert(key, value);
                }
                Err(error) => tracing::warn!(value, error = %error, "parse metadata value"),
            },
            Err(error) => tracing::warn!(key, error = %error, "parse metadata key"),
        }
    }
}

/// OpenTelemetry extractor for gRPC metadata.
pub struct MetadataExtractor<'a>(pub &'a MetadataMap);

impl opentelemetry::propagation::Extractor for MetadataExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key)?.to_str().ok()
    }

    fn keys(&self) -> Vec<&str> {
        self.0
            .keys()
            .map(|k| match k {
                tonic::metadata::KeyRef::Ascii(key) => key.as_str(),
                tonic::metadata::KeyRef::Binary(key) => key.as_str(),
            })
            .collect()
    }
}

/// Server interceptor for extracting trace context and recording trace ID.
#[allow(clippy::result_large_err, clippy::missing_errors_doc)]
pub fn server_interceptor<T>(request: &Request<T>) -> Result<(), tonic::Status> {
    let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&MetadataExtractor(request.metadata()))
    });
    Span::current().set_parent(parent_context);

    let span = Span::current();
    let context = span.context();
    let otel_span = context.span();
    let trace_id = otel_span.span_context().trace_id();
    Span::current().record("trace_id", trace_id.to_string());

    Ok(())
}

/// Client interceptor for injecting trace context.
#[allow(clippy::result_large_err, clippy::missing_errors_doc)]
pub fn client_interceptor<T>(mut request: Request<T>) -> Result<Request<T>, tonic::Status> {
    opentelemetry::global::get_text_map_propagator(|propagator| {
        let context = Span::current().context();
        propagator.inject_context(&context, &mut MetadataInjector(request.metadata_mut()));
    });
    Ok(request)
}
