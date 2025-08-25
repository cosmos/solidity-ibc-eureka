# Tracing and Distributed Observability

Tracing is a framework for instrumenting Rust programs to collect structured, event-based diagnostic information across distributed systems. It enables developers to understand request flows and failure patterns in microservice architectures.

## Core Concepts

### Tracing Fundamentals

**Tracing** enables you to follow a single request as it flows through multiple services, providing end-to-end visibility into distributed operations.

**Subscribers** handle the collection and processing of trace data. A subscriber should typically be a global static instance, though case-specific subscribers can be used when needed. The most common pattern combines stdout output with OpenTelemetry-compatible subscribers.

**Unique Identification**: Every trace must have a unique identifier - either a TraceID (16-byte OpenTelemetry standard) or CCID (Correlation Context Identifier).

### Spans and Context

- **Span**: The smallest unit in a trace, representing a period of time and work in the program
- **Inject**: Taking the current span context, serializing it, and sending it to downstream calls
- **Extract**: Discovering the span context from incoming requests and creating child spans
- **Context Accumulation**: Context can accumulate across service boundaries - be mindful of payload size

### OpenTelemetry Integration

OpenTelemetry provides three major capabilities:

- **Distributed context propagation** - Traces across service boundaries
- **Application tracing** - Internal operation visibility
- **Application metrics** - Quantitative performance data

#### Span Context Components

- **TraceID**: 16-byte array uniquely identifying the entire trace
- **SpanID**: 8-byte array uniquely identifying this span within the trace
- **TraceFlags**: Metadata about trace sampling and processing
- **IsRemote**: Boolean indicating if context was propagated from a remote parent

### Trace Narrative Structure

Traces help build a relational graph showing the complete story of a request:

```
Span name: What happened
Span tags: Why it happened
Span logs: How it happened
```

## Performance Considerations

**Distributed request tracing** works best when operations complete within a reasonable timeframe (typically minutes). For longer-running operations, consider alternative approaches or sampling strategies.

**Overhead**: If every service emits spans with basic attributes that require no runtime overhead (pre-calculated string values), the total overhead per request is minimal - approximately 25 bytes for context headers and negligible CPU cycles for decoding.

## Instrumentation Checklist

### Essential Requirements

- **Span Lifecycle**: Ensure all created spans are properly finished, even during unrecoverable errors
- **Span Kinds**: Set appropriate `SpanKind` for egress and ingress operations
- **Infrastructure Context**: Include identifying information about the underlying infrastructure:
  - Hostname or application instance ID
  - Application server version
  - Region or availability zone
- **Namespace Attributes**: Use consistent namespacing for attributes
- **Unit Specification**: Attributes with numeric values must include units in the key name
  - ✅ Good: `payload_size_bytes`, `latency_seconds`
  - ❌ Bad: `payload_size`, `latency`
- **Version Tracking**: Version attributes are critical for debugging across deployments

### Debugging Considerations

When implementing tracing, consider:

- **Unknown unknowns**: What information might be needed during incident response?
- **Performance impact**: How will this instrumentation affect production performance?
- **Error context**: What information is needed when things go wrong?

> **Conditional Instrumentation**: In Rust, you can use conditional fields in the `#[instrument]` macro based on error conditions by leveraging the `err` parameter and field visibility controls.

## Rust Implementation Guide

### Essential Dependencies

The following crates provide a complete tracing solution:

- **`tracing`** - Core instrumentation framework for Rust code
- **`tracing-subscriber`** - Event listening, filtering, and export configuration
- **`tracing-log`** - Collects log events from third-party libraries into your subscriber

  ```rust
  LogTracer::init().expect("Failed to set logger");
  ```

- **`opentelemetry`** - OpenTelemetry API-level interfaces for tracing and spans
- **`opentelemetry_sdk`** - OpenTelemetry SDK implementation
- **`tracing-opentelemetry`** - Compatibility layer between tracing and OpenTelemetry
- **`opentelemetry-otlp`** - OTLP protocol implementation for exporting to Jaeger and other backends

### Critical Setup Rules

⚠️ **Important**: Any trace events generated outside an active subscriber context will be lost.

⚠️ **Library Constraint**: Libraries should **never** install a global subscriber using `set_global_default()`. This will cause conflicts when executables attempt to configure their own subscribers.

💡 **Runtime Configuration**: Tracing filters can be changed during runtime using the [reload functionality](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/reload/index.html).

## gRPC Tracing Implementation

### Automatic Trace Context Propagation

The relayer uses automatic trace context extraction/injection with interceptors from `packages/relayer/lib/src/utils/tracing_layer.rs`, eliminating manual calls in each service method.

#### Production Interceptor Implementation

```rust
use opentelemetry::trace::TraceContextExt;
use tonic::{metadata::MetadataMap, Request, Status};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

/// Tonic interceptor function that automatically extracts trace context and records trace ID.
#[allow(clippy::result_large_err)]
pub fn tracing_interceptor<T>(request: Request<T>) -> Result<Request<T>, Status> {
    // Extract parent context from gRPC metadata
    let parent_context = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&MetadataExtractor(request.metadata()))
    });

    // Set parent context for current span
    Span::current().set_parent(parent_context);

    // Extract and record the trace ID for logging correlation
    let span = Span::current();
    let context = span.context();
    let otel_span = context.span();
    let trace_id = otel_span.span_context().trace_id();
    span.record("trace_id", trace_id.to_string());

    Ok(request)
}
```

#### Metadata Extractors and Injectors

```rust
/// OpenTelemetry extractor for gRPC metadata (server-side)
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

/// OpenTelemetry injector for gRPC metadata (client-side)
pub struct MetadataInjector<'a>(pub &'a mut MetadataMap);

impl opentelemetry::propagation::Injector for MetadataInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        match tonic::metadata::MetadataKey::from_bytes(key.as_bytes()) {
            Ok(key) => match tonic::metadata::MetadataValue::try_from(&value) {
                Ok(value) => {
                    self.0.insert(key, value);
                }
                Err(error) => tracing::debug!(value, error = %error, "parse metadata value"),
            },
            Err(error) => tracing::debug!(key, error = %error, "parse metadata key"),
        }
    }
}
```

#### Server Configuration

```rust
// In packages/relayer/core/src/builder.rs
use ibc_eureka_relayer_lib::utils::tracing_layer::tracing_interceptor;

// Apply interceptor automatically to all gRPC services
Server::builder()
    .add_service(RelayerServiceServer::with_interceptor(
        relayer,
        tracing_interceptor,  // Automatic trace propagation
    ))
    .add_service(reflection_service)
    .serve(socket_addr)
    .await?;
```

#### Service Method Instrumentation

```rust
// All service methods use this pattern:
#[instrument(
    skip(self, request),
    fields(
        src_chain = %request.get_ref().src_chain,
        dst_chain = %request.get_ref().dst_chain,
        trace_id = tracing::field::Empty  // Reserved for interceptor
    )
)]
async fn relay_by_tx(
    &self,
    request: Request<api::RelayByTxRequest>,
) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
    let inner_request = request.get_ref();
    let src_chain = inner_request.src_chain.clone();
    let dst_chain = inner_request.dst_chain.clone();

    // Metrics tracking with tracing integration
    crate::metrics::track_metrics("relay_by_tx", &src_chain, &dst_chain, || async move {
        self.get_module(&inner_request.src_chain, &inner_request.dst_chain)?
            .relay_by_tx(request)
            .await
            .map_err(|e| {
                error!(error = %e, "Relay by tx request failed");
                tonic::Status::internal("Failed to relay by tx. See logs for more details.")
            })
    })
    .await
}
```

### Benefits of This Pattern

- **Zero Manual Work**: Impossible to forget trace context extraction
- **Consistent Correlation**: Every span gets proper trace_id automatically
- **Clean Service Code**: Business logic stays focused, no observability boilerplate
- **Universal Coverage**: Works for all gRPC methods without modification
- **Error Resilience**: Graceful handling of malformed trace headers

### Span Deep Dive

A span is a lightweight handle consisting of an ID and reference to the current subscriber. Conceptually, it serves as a key to the subscriber's internal storage.

#### Span Creation Process

1. **Metadata Generation**: The macro builds static metadata for the span
2. **Subscriber Interaction**: Calls `Subscriber::new_span(&Attributes)`
3. **Layer Processing**: All layers receive `on_new_span` callbacks with the attributes
4. **Handle Return**: A `tracing::Span` handle is returned containing the span ID and cloned dispatch

⚠️ **Important**: A span does not become current until it's explicitly entered.

#### Manual Span Creation

```rust
pub async fn subscribe(/* */) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Adding a new subscriber",
        %request_id,
        subscriber_email = %form.email,
        subscriber_name = %form.name
    );
    // ⚠️ Using `enter()` in async functions is dangerous!
    let _guard = request_span.enter();
}
```

#### Span Stack and Threading

- **Stack Behavior**: `span.enter()` pushes the span onto the current span stack
- **Scope**: The stack of entered spans is thread-local (or task-local with `#[instrument]`)
- **Thread Safety**: `tracing::Span` itself is `Send + Sync`, but `current_span()` is thread-local
- **Async Caution**: Manually entering spans in async functions can lead to incorrect context if the future migrates between threads

#### Span Lifecycle

- **Multiple Entry/Exit**: You can enter or exit a span multiple times during its lifetime
- **Final Closure**: Once a span is closed, it cannot be reopened
- **Automatic Management**: Use `#[instrument]` to avoid manual span management in async code

### Subscriber Architecture

The `tracing::Subscriber` trait defines the core interface for trace collection. The `Registry` is the primary implementation provided by `tracing-subscriber`.

**Key Implementations:**

- **Registry**: Standard subscriber for collecting and organizing spans
- **tokio-console**: Specialized subscriber for debugging and profiling async applications (requires `tokio-unstable` feature)

### Registry Responsibilities

The Registry acts as the foundation layer that:

- **Metadata Storage**: Maintains span metadata and attributes
- **Relationship Tracking**: Records parent-child relationships between spans
- **Lifecycle Management**: Tracks which spans are active, entered, and closed
- **Delegation**: Routes events to appropriate layers for processing

**Note**: The Registry itself does not record traces - it coordinates with layers that handle the actual recording and export.

## Best Practices

### Code Instrumentation

**Always include line numbers** in trace output for easier debugging:

```rust
fmt::layer()
    .with_line_number(true)
    .with_file(true)
```

**Never leak credentials** in trace data:

```rust
#[instrument(skip(self, credential), fields(username = %credential.username))]
async fn authenticate(&self, credential: &Credentials) -> Result<User, AuthError> {
    // credential details are skipped, only username is recorded
}
```

**Use `secrecy::Secret`** to automatically mask sensitive data:

```rust
use secrecy::Secret;
let password: Secret<String> = Secret::new("sensitive_data".to_string());
```

### Library Guidelines

Libraries should:

- **Only use the `tracing` crate** for instrumentation
- **Provide useful information** to downstream consumers through structured spans
- **Never install global subscribers** - leave that to the application binary

Reference: [Tracing Library Guidelines](https://github.com/tokio-rs/tracing)

### Advanced Patterns

**Deferred Field Population** using `tracing::field::Empty`:

This is helpful for context propagation.

```rust
#[instrument(fields(
    trace_id = tracing::field::Empty,
    result = tracing::field::Empty
))]
async fn process_request() -> Result<String, Error> {
    // Fields get populated later by interceptors or during execution
    let result = do_work().await?;
    Span::current().record("result", &result);
    Ok(result)
}
```

**Rich Error Context** with `tracing-error` and `eyre`:

```rust
tracing_subscriber::registry()
    .with(filter_layer)
    .with(fmt_layer)
    .with(tracing_error::ErrorSubscriber::default())
    .init();

color_eyre::install()?;
```

**Human-Readable Development Traces**:

```rust
fmt::layer()
    .pretty()
    .with_target(true)
    .with_line_number(true)
```

## Anti-patterns

### Common Mistakes to Avoid

#### ❌ Over-logging instead of spanning

- When in doubt, create a new span rather than additional log statements
- Spans provide structure and timing information that logs cannot

#### ❌ Including `self` in instrumentation unnecessarily

- Skip `self` from instrumentation unless the instance provides meaningful context
- Use `skip(self)` to avoid noise in traces

#### ❌ Timestamp overhead in development

- Disable timestamp generation in non-production environments for uncluttered logs
- Enable only when needed for debugging

#### ❌ Double error handling

- Automatic error logging can violate the "handle errors once" principle:

```rust
// ❌ This automatically logs errors, potentially duplicating error handling
#[instrument(level = "trace", err(Debug/Display))]
async fn risky_operation() -> Result<(), Error> {
    // If this fails, it gets logged automatically AND returned
}
```

### Development Tools Consideration

**Enhanced Error Reporting**: Use [`color-eyre`](https://docs.rs/color-eyre/0.6.5/color_eyre/) for rich error context in development.
Eyre is a fork of `anyhow` and `color-eyre` gives nice looking error messages.

## Implementation Examples

### Basic Subscriber Setup

```rust
pub fn get_subscriber(
    name: String,
    env_filter: String,
    sink: Sink,
) -> impl Subscriber + Send + Sync
where
    Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(env_filter));
    let formatting_layer = BunyanFormattingLayer::new(name, sink);
    Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
}

pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    LogTracer::init().expect("Failed to set logger");
    set_global_default(subscriber).expect("Failed to set subscriber");
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let subscriber = get_subscriber("my_service".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);
    // Application logic here
}
```

### Test Environment Setup

```rust
use once_cell::sync::Lazy;

// Ensure tracing is initialized only once across tests
static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    let sink = if std::env::var("MUTE_TEST_LOG").is_ok() {
        std::io::sink()
    } else {
        std::io::stdout()
    };

    let subscriber = get_subscriber(subscriber_name, default_filter_level, sink);
    init_subscriber(subscriber);
});

pub fn init_test_tracing() {
    Lazy::force(&TRACING);
}

#[tokio::test]
async fn my_test() {
    init_test_tracing();
    // Test logic with tracing enabled
}
```

## Production Implementation

### Observability Configuration Structure

The relayer uses a centralized observability configuration that replaces scattered logging settings:

```rust
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// The log level to use (trace, debug, info, warn, error)
    pub level: String,
    /// Whether to use OpenTelemetry for distributed tracing and logging
    pub use_otel: bool,
    /// The service name to use for OpenTelemetry
    pub service_name: String,
    /// The OpenTelemetry collector endpoint (typically OTLP gRPC)
    pub otel_endpoint: Option<String>,
}
```

### Complete Production Setup

The relayer implements a production-ready observability stack in [`programs/relayer/src/observability.rs`](file:///Users/gg/code/vibes/solidity-ibc-eureka/tamjid-tracing/programs/relayer/src/observability.rs):

```rust
pub fn init_observability(config: &ObservabilityConfig) -> Result<ObservabilityGuard> {
    let (otel_tracer_provider, otel_logger_provider) = if config.use_otel {
        match setup_opentelemetry(config) {
            Ok((tracer, tracer_provider, logger_provider)) => {
                // Configure subscriber with OpenTelemetry layers
                let subscriber = Registry::default()
                    .with(EnvFilter::new(config.level().as_str().to_lowercase()))
                    .with(create_fmt_layer())
                    .with(tracing_opentelemetry::layer().with_tracer(tracer))
                    .with(create_otel_log_layer(logger_provider.clone()));
                
                try_init_subscriber(subscriber)?;
                (Some(tracer_provider), Some(logger_provider))
            }
            Err(e) => {
                eprintln!("OpenTelemetry disabled: {e}");
                // Fallback to console-only logging
                let subscriber = Registry::default()
                    .with(EnvFilter::new(config.level().as_str().to_lowercase()))
                    .with(create_fmt_layer());
                try_init_subscriber(subscriber)?;
                (None, None)
            }
        }
    } else {
        // Console-only configuration
        let subscriber = Registry::default()
            .with(EnvFilter::new(config.level().as_str().to_lowercase()))
            .with(create_fmt_layer());
        try_init_subscriber(subscriber)?;
        (None, None)
    };

    Ok(ObservabilityGuard { otel_tracer_provider, otel_logger_provider })
}
```

### Automatic gRPC Interceptor Integration

The relayer automatically applies trace context propagation to all gRPC services using the interceptor pattern from [`packages/relayer/lib/src/utils/tracing_layer.rs`](file:///Users/gg/code/vibes/solidity-ibc-eureka/tamjid-tracing/packages/relayer/lib/src/utils/tracing_layer.rs):

```rust
// In packages/relayer/core/src/builder.rs
Server::builder()
    .add_service(RelayerServiceServer::with_interceptor(
        relayer,
        tracing_interceptor,  // Automatic trace context extraction
    ))
    .serve(socket_addr)
    .await?;
```

All service methods use deferred field population for trace IDs:

```rust
#[instrument(
    skip(self, request),
    fields(
        src_chain = %request.get_ref().src_chain,
        dst_chain = %request.get_ref().dst_chain,
        trace_id = tracing::field::Empty  // Populated by interceptor
    )
)]
async fn relay_by_tx(
    &self,
    request: Request<api::RelayByTxRequest>,
) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
    // Business logic - trace_id automatically populated
}
```

### OTLP Export Configuration

When `use_otel` is true, both traces and logs are exported via OTLP gRPC:

```json
{
  "observability": {
    "level": "info",
    "use_otel": true,
    "service_name": "ibc-eureka-relayer",
    "otel_endpoint": "http://localhost:4317"
  }
}
```

The implementation bridges `tracing` events to OpenTelemetry logs using an appender layer while retaining console output for development.

### Resource Attributes

All traces and logs include standardized resource attributes:

- `service.name`: From configuration (e.g., "ibc-eureka-relayer")
- `service.version`: Automatically set to `CARGO_PKG_VERSION`
- Custom span attributes: `src_chain`, `dst_chain`, `trace_id`

### Error Handling and Fallbacks

The observability system includes robust error handling:

- If OpenTelemetry setup fails, gracefully falls back to console-only logging
- Always maintains console output for development visibility
- Provides clear error messages when OTLP endpoints are unreachable

## Local observability for e2e (Grafana + Tempo + Prometheus)

This project includes a local Grafana observability stack for development and e2e testing located in [`scripts/local-grafana-stack/`](file:///Users/gg/code/vibes/solidity-ibc-eureka/tamjid-tracing/scripts/local-grafana-stack/).

### Start the local stack

```bash
cd scripts/local-grafana-stack
docker compose up -d
```

**Services provided:**
- **Grafana**: http://localhost:3002 (anonymous access enabled)
- **Tempo** (traces backend): internal service at `tempo:3200` (configured as Grafana datasource)
- **Prometheus**: http://localhost:9090 (metrics collection)
- **Grafana Alloy** (collector): OTLP gRPC on `0.0.0.0:4317` and HTTP on `4318`

### Enable relayer tracing to local stack in e2e

Set the environment variable before running e2e tests:

```bash
export ENABLE_LOCAL_OBSERVABILITY=true
```

**Automatic behavior when enabled:**

1. **Observability config** in generated relayer `config.json` becomes:
   ```json
   {
     "observability": {
       "level": "<from RUST_LOG or 'info' if unset>",
       "use_otel": true,
       "service_name": "ibc-eureka-relayer",
       "otel_endpoint": "http://127.0.0.1:4317"
     }
   }
   ```

2. **Log level control** via `RUST_LOG` environment variable:
   ```bash
   export RUST_LOG=debug  # Optional: set different log level
   ```

3. **Prometheus metrics** served at `http://0.0.0.0:9000/metrics` and automatically scraped

### Using the stack for development

**Generate traces with e2e tests:**
```bash
# Start the observability stack
cd scripts/local-grafana-stack && docker compose up -d

# Enable observability and run tests
export ENABLE_LOCAL_OBSERVABILITY=true
export RUST_LOG=info
just e2e-test
```

**View traces in Grafana:**
1. Open Grafana at http://localhost:3002
2. Navigate to **Explore** → **Tempo datasource**
3. Run a trace search for recent activity
4. Look for spans with `service.name = ibc-eureka-relayer`

**Query traces by operation:**
- Search by service: `{service.name="ibc-eureka-relayer"}`
- Filter by operation: `{span.name="relay_by_tx"}`
- Time-based queries: Use the time picker for specific ranges

### Network configuration notes

- **e2e relayer runs on host**: Uses `http://127.0.0.1:4317` to reach Alloy collector in Docker
- **OTLP transport**: gRPC on port 4317 (configured in [`config.alloy`](file:///Users/gg/code/vibes/solidity-ibc-eureka/tamjid-tracing/scripts/local-grafana-stack/config.alloy))
- **HTTP alternative**: Available on port 4318 if needed
- **Metrics endpoint**: Prometheus scrapes from `host.docker.internal:9000`

### Troubleshooting

**No traces appearing:**
1. Verify `ENABLE_LOCAL_OBSERVABILITY=true` is set
2. Check relayer logs for OTLP connection errors
3. Ensure Docker stack is running: `docker compose ps`

**Connection issues:**
1. Verify port 4317 is accessible: `nc -zv 127.0.0.1 4317`
2. Check Alloy logs: `docker compose logs alloy`
3. Confirm relayer is using OTLP: look for "OpenTelemetry initialized" in logs

**Performance impact:**
- Tracing overhead is minimal for development
- Disable with `ENABLE_LOCAL_OBSERVABILITY=false` for performance testing
