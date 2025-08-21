//! Tonic interceptor for automatic trace context extraction and trace ID recording.

use super::correlation::MetadataExtractor;
use opentelemetry::trace::TraceContextExt;
use tonic::{Request, Status};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Tonic interceptor function that automatically extracts trace context and records trace ID.
/// This can be used with `.interceptor()` on tonic services.
///
/// # Errors
/// This function never returns an error, but uses `Result` to match tonic's interceptor signature.
#[allow(clippy::result_large_err)]
pub fn tracing_interceptor<T>(request: Request<T>) -> Result<Request<T>, Status> {
    // Extract parent context from gRPC metadata
    let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&MetadataExtractor(request.metadata()))
    });

    // Set the parent context for the current span
    Span::current().set_parent(parent_context);

    // Extract and record the trace ID
    let span = Span::current();
    let context = span.context();
    let otel_span = context.span();
    let trace_id = otel_span.span_context().trace_id();
    Span::current().record("trace_id", trace_id.to_string());

    Ok(request)
}
