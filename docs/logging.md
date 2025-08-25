# Logging and best practices

## Key principles
Keep logs simple, structured, and valuable for on-call debugging. This page captures only the essentials so teams can adopt a consistent approach quickly.

- Purpose: logs exist to explain what happened and why, especially when something fails. Optimize for on-call readability and machine parsing.
- Think in events: one log captures a single event with context (including correlation fields)
- Never log secrets: just don't
- Practical levels: prefer INFO; use DEBUG sparingly; only ERROR for failures that need attention.

## Log levels
- DEBUG: development-time details and deep diagnosis. Usually disabled or sampled in prod.
- INFO: normal operations and key state transitions (startup, readiness, completed request, retries).
  - Prefer INFO over DEBUG, because that is what someone on-call will be able to read.
- ERROR: errors that caused failure, dropped work, or user-visible impact. Include actionable context.

## Correlation
Correlation fields are what we use to find related information. 
It can be any information that identifies an area we care about, such as a specific request, module, environment, etc.
It enables us to compile logs and metrics for one or more attributes, helping us narrow down the search for information or a signal.

### Traces vs spans
A trace is the entire journey of a request as it flows through multiple services, components, and operations.
Typically identified by an initiating request ID.

Spans are sub-sections _in_ a span is more like a single unit of work — it represents one operation in your system.
Example: A database query, an HTTP request to another service, or a function execution.
Super helpful if you want to figure out where time is being spent in a request to find performance bottlenecks or other issues related to timing.

### Correlation fields

To make correlation possible, always include available correlation fields.
They should be either set up at service launch (environment, service name, versions) or automated by the framework.

**Minimum correlation fields:**
- `trace_id`: Request correlation ID (UUID or OpenTelemetry trace ID)
- `timestamp`: RFC 3339 UTC (handled automatically by logging frameworks)
- `service_name`: Current service identifier
- `service_version`: Deployed code version
- `environment`: dev/staging/prod
- `span_id`: when available

## Logging errors

It is important to only handle errors once. Errors can become overly distracting if they are handled multiple times (logging an error and returning an error both count as handling the error).
Typically an error should be logged only when it is decided that the error will either be returned to the client (client here means an external api client, not returning to a calling function kind of client), or when it is decided that the error will be discarded (e.g. a goroutine that does not return errors to the parent function).

## Rules
- Use structured logging (JSON) to stdout/stderr; avoid multiline messages.
- Use OpenTelemetry semantic conventions where applicable and keep field names consistent across services.
- Include correlation fields and basic resource fields (service.name, service.version, environment).
- Use RFC 3339 UTC timestamps and disable ANSI colors in production logs.

## Language-specific guides

### Rust

The relayer implements production-ready logging using the `tracing` ecosystem with OpenTelemetry export capabilities.

#### Observability Configuration Structure

The relayer uses a centralized observability configuration that handles logging, tracing, and metrics:

```rust
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// The log level to use (trace, debug, info, warn, error)
    pub level: String,
    /// Whether to use OpenTelemetry for distributed tracing and log export
    pub use_otel: bool,
    /// The service name to use for OpenTelemetry
    pub service_name: String,
    /// The OpenTelemetry collector endpoint (OTLP gRPC)
    pub otel_endpoint: Option<String>,
}
```

##### Configuration Examples

**Development (console-only):**
```json
{
  "observability": {
    "level": "debug",
    "use_otel": false,
    "service_name": "ibc-eureka-relayer"
  }
}
```

**Production (with OTLP export):**
```json
{
  "observability": {
    "level": "info", 
    "use_otel": true,
    "service_name": "ibc-eureka-relayer",
    "otel_endpoint": "http://otel-collector:4317"
  }
}
```

**E2E testing (local Grafana stack):**
```json
{
  "observability": {
    "level": "info",
    "use_otel": true,
    "service_name": "ibc-eureka-relayer",
    "otel_endpoint": "http://127.0.0.1:4317"
  }
}
```

#### Dependencies

Add these to your `Cargo.toml`:

```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
tracing-opentelemetry = "0.26"
opentelemetry = { version = "0.26", features = ["trace"] }
opentelemetry_sdk = { version = "0.26", features = ["trace"] }
opentelemetry-otlp = { version = "0.26", features = ["tonic"] }
opentelemetry-appender-tracing = "0.26"  # For log export
```

#### Implementation Pattern

**1. Observability Initialization**

```rust
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Registry};
use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

pub fn init_observability(config: &ObservabilityConfig) -> Result<ObservabilityGuard> {
    if config.use_otel {
        // Production setup with OTLP export
        let resource = Resource::builder()
            .with_attributes(vec![
                KeyValue::new("service.name", config.service_name.clone()),
                KeyValue::new("service.version", env!("CARGO_PKG_VERSION")),
            ])
            .build();

        let tracer = setup_otlp_tracer(config, resource.clone())?;
        let logger_provider = setup_otlp_logger(config, resource)?;

        let subscriber = Registry::default()
            .with(EnvFilter::new(&config.level))
            .with(create_fmt_layer())
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .with(create_otel_log_layer(logger_provider.clone()));

        subscriber.try_init()?;
        Ok(ObservabilityGuard { logger_provider: Some(logger_provider) })
    } else {
        // Development setup (console only)
        let subscriber = Registry::default()
            .with(EnvFilter::new(&config.level))
            .with(create_fmt_layer());

        subscriber.try_init()?;
        Ok(ObservabilityGuard { logger_provider: None })
    }
}
```

**2. Structured Logging with Correlation**

```rust
use tracing::{info, error, instrument};

#[instrument(
    skip(self, request),
    fields(
        // Business context
        src_chain = %request.get_ref().src_chain,      // "cosmoshub-4"
        dst_chain = %request.get_ref().dst_chain,      // "osmosis-1"  
        src_client_id = %request.get_ref().src_client_id, // "07-tendermint-0"
        
        // Correlation context
        trace_id = tracing::field::Empty,  // Populated by interceptor
        
        // Resource attributes (set at initialization)
        service_name = "ibc-eureka-relayer",
        service_version = env!("CARGO_PKG_VERSION"),
        environment = std::env::var("ENVIRONMENT").unwrap_or("dev".to_string()),
    )
)]
async fn relay_by_tx(request: Request<RelayByTxRequest>) -> Result<Response<RelayByTxResponse>> {
    info!("Starting relay by tx operation");
    
    match process_relay(&request).await {
        Ok(result) => {
            info!(packets_relayed = result.count, "Relay completed successfully");
            Ok(result)
        }
        Err(e) => {
            error!(error = %e, "Relay operation failed");
            Err(e)
        }
    }
}
```

**3. Error Handling Pattern**

```rust
// ✅ Handle errors once - log when converting to user-facing error
self.get_module(&src_chain, &dst_chain)?
    .relay_by_tx(request)
    .await
    .map_err(|e| {
        error!(error = %e, "Relay by tx request failed"); // Log here
        tonic::Status::internal("Failed to relay by tx. See logs for more details.")
    })

// ❌ Avoid double logging
// Don't log here AND return error - that's double handling
```

**4. OTLP Log Export Setup**

```rust
fn setup_otlp_logger(config: &ObservabilityConfig) -> Result<SdkLoggerProvider> {
    let mut exporter_builder = opentelemetry_otlp::LogExporter::builder().with_tonic();
    
    if let Some(endpoint) = &config.otel_endpoint {
        exporter_builder = exporter_builder.with_endpoint(endpoint);
    }
    
    let provider = SdkLoggerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter_builder.build()?)
        .build();
        
    Ok(provider)
}

fn create_otel_log_layer(provider: SdkLoggerProvider) -> impl Layer<Registry> {
    opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(&provider)
}
```

**5. Local Development Tips**

```rust
// Pretty console output for development
fn create_fmt_layer() -> impl Layer<Registry> {
    tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_line_number(true)
        .with_file(true)
        .pretty()  // Remove in production
}

// Environment-based configuration
let level = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string());
```

#### Best Practices for Rust

- **Use structured fields**: `info!(user_id = %id, action = "login", "User authenticated")`
- **Avoid format strings in span names**: Use `#[instrument(name = "process_request")]`
- **Skip sensitive data**: `#[instrument(skip(password, credentials))]`
- **Use proper error formatting**: `error = %err` for Display, `error = ?err` for Debug
- **Resource attributes**: Always include service name and version
- **Console + OTLP**: Keep both for development visibility and production export

#### Common Patterns

**Conditional logging:**
```rust
if let Err(e) = non_critical_operation().await {
    tracing::warn!(error = %e, "Non-critical operation failed, continuing");
}
```

**Context propagation:**
```rust
let span = tracing::info_span!("background_task", task_id = %id);
let _guard = span.enter();
// All logs in this scope will include task_id
```

**Performance-sensitive logging:**
```rust
// Use debug level for expensive-to-compute context
tracing::debug!(expensive_data = ?compute_debug_info(), "Debug context");
```

#### Local Development with Observability Stack

For development and testing, use the local Grafana stack to see logs alongside traces and metrics.

**Quick Setup:**

```bash
# Start the observability stack
cd scripts/local-grafana-stack
docker compose up -d

# Enable observability in e2e tests
export ENABLE_LOCAL_OBSERVABILITY=true
export RUST_LOG=debug  # Optional: more verbose logging

# Run tests with full observability
just e2e-test
```

**Viewing Logs in Grafana:**

1. **Access Grafana**: http://localhost:3002
2. **Navigate to Explore** → **Loki datasource** (if configured)
3. **Query logs**: `{service_name="ibc-eureka-relayer"}`
4. **Filter by level**: `{service_name="ibc-eureka-relayer"} |= "ERROR"`
5. **Correlate with traces**: Use `trace_id` to find related spans in Tempo

**Log Export Behavior when `ENABLE_LOCAL_OBSERVABILITY=true`:**

- **Console output**: Pretty-formatted logs for development
- **OTLP export**: Structured logs sent to Alloy collector on `localhost:4317`
- **Correlation**: Automatic trace_id injection for cross-reference with traces
- **Level control**: Respects `RUST_LOG` environment variable

**Development Tips:**

```bash
# View structured JSON logs
export RUST_LOG=debug
cargo run | jq .

# Find trace ID in logs, then search Tempo
grep "trace_id" logs.json | jq -r '.trace_id' | head -1
# Copy trace_id and search in Grafana Tempo

# Disable observability for benchmarking
export ENABLE_LOCAL_OBSERVABILITY=false
export RUST_LOG=warn
```

