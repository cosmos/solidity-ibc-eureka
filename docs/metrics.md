# Metrics and best practices

The goal of metric collection is to provide data-driven insights into the inner workings of your services. 

The particular goals we aim to achieve, and that you should keep in mind while implementing metrics, are:
- Debugging
  - Rapid issue identification through structured error metrics and performance bottlenecks
- Operational Health
  - Proactive monitoring of service availability and resource utilization  
- Performance Optimization
  - Data-driven insights into latency patterns and throughput characteristics
- Alerting
  - Automated incident response based on service-level indicators

## Key principles

- Every metric has a purpose
  - Every metric needs to be documented as to why it exists, and what it is meant to capture (business or technical goal).
- RED metrics
  - Start with RED metrics (Rate, Errors, Duration) for every service
- Correlate with logs and traces
  - Correlate metrics with logs and traces using correlation IDs for effective debugging
- Alert on symptoms, not causes
  - Focus on metrics that have an impact, not internal system states that may or may not cause any real-world impact
- Open Standards
  - Use open standards like OpenTelemetry for vendor neutrality
- Include metrics in the design phase
  - Implement observability by design - include metrics requirements from service conception, don't just tack 'em on later.
- Use established conventions and standards
  - Use consistent naming conventions across all services _and_ teams

## What to measure

- Request lifecycle
  - Every request, message, or transaction moving through your system. 
  - Track the full lifecycle: received, processing, succeeded, failed, timed out.
- Connection health
  - Track whether your service can talk to what it needs to: internal services, 3rd party services, RPC nodes, databases. 
  - Include connection state, latency to dependencies, and time until critical resources expire (like IBC clients).
- Error patterns
  - Group errors into categories that make sense for debugging: timeout errors, validation failures, insufficient funds, and network issues. 
  - You want to spot patterns, not track every unique error message.
- Performance bottlenecks
  - Request latencies at different stages of processing, queue depths, batch sizes, and throughput rates. 
  - Focus on the operations that directly impact user experience or system stability.
- Resource consumption
  - Memory usage, CPU utilization, goroutines/threads, file descriptors.
  - Track both your service's consumption and any limits you're operating under.

## When to measure

### Add metrics when:
- You're implementing a new critical path that could fail
- You've had an incident and realized you were flying blind
- You need to validate a critical performance assumption
- You're adding SLAs or need to track service level objectives
- There's a resource that could be exhausted (connections, memory, etc.)

### Skip metrics when:
- The information is already available through logs or traces
- You're tracking something "just in case" with no clear use case
- The cardinality would be unbounded (user IDs, transaction hashes, timestamps)
- It duplicates an existing metric at a slightly different granularity

## Conventions and best practices

### Metric names
Use this format: `namespace_subsystem_name_unit`

Good examples:
- `solidity_ibc_relayer_packets_sent_total`
- `solidity_ibc_relayer_client_expiry_seconds`
- `solidity_ibc_relayer_rpc_duration_seconds`

Bad examples:
- `PacketsSent` - wrong case, no namespace
- `relayer_memory_mb` - use base units (bytes, not MB)
- `connection_count` - should be `connections_total` if it's a counter

### Labels

> _Cardinality_ is about how many unique values a metric's labels (or dimensions) can take.
> For example: if you have a metric for HTTP requests with the labels `status_code` and `method`,
> and `status_code` has 5 possible values and `method` has 3 possible values, the cardinality for those combined labels is 5 × 3 = 15 unique combinations.

- Labels should have bounded, predictable values:
  - `chain_id`: "cosmoshub-4", "osmosis-1"
  - `status`: "success", "timeout", "invalid_proof"
- Keep cardinality under 100 per metric
  - If you can't list all possible values, don't make it a label!

### Other rules

- Never expose sensitive data
  - No private keys, passwords, or internal IPs in metrics
- Always use seconds for time - Not milliseconds, not minutes. Seconds.
- Counters only go up
  - Never reset a counter to zero. Let Prometheus handle resets.
- One metric, one meaning
  - Don't reuse the same metric name for different things
- Base units only
  - Bytes not kilobytes, seconds not milliseconds

## Metric types

### Counter
A number that only goes up. Perfect for counting things that happen - requests, errors, bytes processed. You'll usually query these with `rate()` or `increase()`.

When to use: Total requests served, packets sent, errors encountered, and bytes transferred.

### Gauge
A value that can go up or down. Current state of something - active connections, queue depth, temperature, and available memory.

When to use: Active connections, queue sizes, current memory usage, and last successful run timestamp.

### Histogram
Sample observations and count them in buckets. Great for latencies and sizes where you care about percentiles. More expensive than counters and gauges.

When to use: Request durations, response sizes, processing times - anything where you need percentiles.

### Summary
Similar to a histogram, but calculates quantiles on the client side. Generally, histograms are preferred because they can be aggregated.

When to use: Rarely. You can use histograms instead unless you have a specific reason.

## Language-specific guides

### Rust

The relayer implements comprehensive metrics using the Prometheus crate with automatic collection and HTTP export.

#### RED Metrics Implementation

The relayer implements the RED pattern (Rate, Errors, Duration) for comprehensive service monitoring:

**Rate:**
```rust
// Total request volume
static REQUEST_COUNTER: LazyLock<Counter> = LazyLock::new(|| {
    register_counter!("eureka_relayer_request_total", "Total number of requests").unwrap()
});
```

**Errors:**
```rust
// Error classification by gRPC status codes
static RESPONSE_CODE: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "eureka_relayer_response_codes",
        "Response Codes", 
        &["method", "src_chain", "dst_chain", "status_code"]
    ).unwrap()
});
```

**Duration:**
```rust
// Response time distribution with meaningful labels
static RESPONSE_TIME: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        "eureka_relayer_response_time_seconds",
        "Response time in seconds",
        &["method", "src_chain", "dst_chain"]
    ).unwrap()
});
```

This provides complete observability for:
- **Rate**: `rate(eureka_relayer_request_total[5m])` - requests per second
- **Errors**: `rate(eureka_relayer_response_codes{status_code!="0"}[5m])` - error rate
- **Duration**: `histogram_quantile(0.95, rate(eureka_relayer_response_time_seconds_bucket[5m]))` - 95th percentile latency

#### Dependencies

Add to your `Cargo.toml`:

```toml
[dependencies]
prometheus = { version = "0.13", features = ["push"] }
tokio = { version = "1.0", features = ["full"] }
warp = "0.3"  # For HTTP metrics endpoint
```

#### Metrics Implementation Pattern

**1. Define Metrics with Static Registration**

```rust
use prometheus::{
    register_counter, register_histogram_vec, register_int_counter_vec, register_int_gauge,
    Counter, HistogramVec, IntCounterVec, IntGauge,
};
use std::sync::LazyLock;

/// Total number of requests across all endpoints
pub static REQUEST_COUNTER: LazyLock<Counter> = LazyLock::new(|| {
    register_counter!("eureka_relayer_request_total", "Total number of requests").unwrap()
});

/// Response time in seconds, by method and chain pair
pub static RESPONSE_TIME: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        "eureka_relayer_response_time_seconds",
        "Response time in seconds",
        &["method", "src_chain", "dst_chain"]
    )
    .unwrap()
});

/// Response codes by method, chain pair, and status
pub static RESPONSE_CODE: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "eureka_relayer_response_codes",
        "Response Codes",
        &["method", "src_chain", "dst_chain", "status_code"]
    )
    .unwrap()
});

/// Current number of active connections/requests
pub static CONNECTED_CLIENTS: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!("eureka_relayer_connected_clients", "Connected clients").unwrap()
});
```

**2. Metrics Tracking Middleware**

```rust
use std::time::Instant;
use tonic::{Response, Status};

/// Generic metrics tracking middleware for service calls
pub async fn track_metrics<F, Fut, R>(
    method: &str,
    src_chain: &str, 
    dst_chain: &str,
    f: F,
) -> Result<Response<R>, Status>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Response<R>, Status>>,
{
    let timer = Instant::now();
    CONNECTED_CLIENTS.inc();
    REQUEST_COUNTER.inc();

    let result = f().await;
    
    // Record metrics based on result
    let status_code: isize = match &result {
        Ok(_) => 0,                    // Success
        Err(status) => status.code() as isize,  // gRPC error code
    };

    // Record response time
    RESPONSE_TIME
        .with_label_values(&[method, src_chain, dst_chain])
        .observe(timer.elapsed().as_secs_f64());

    // Record response code
    RESPONSE_CODE
        .with_label_values(&[method, src_chain, dst_chain, &status_code.to_string()])
        .inc();

    CONNECTED_CLIENTS.dec();
    result
}
```

**3. HTTP Metrics Endpoint**

```rust
use prometheus::{Encoder, TextEncoder};
use warp::Filter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Start metrics server in background
    tokio::spawn(async {
        let metrics_route = warp::path("metrics").map(|| {
            let encoder = TextEncoder::new();
            let metric_families = prometheus::gather();
            let mut buffer = Vec::new();
            encoder.encode(&metric_families, &mut buffer).unwrap();
            String::from_utf8(buffer).unwrap()
        });

        tracing::info!("Metrics available at http://0.0.0.0:9000/metrics");
        warp::serve(metrics_route).run(([0, 0, 0, 0], 9000)).await;
    });

    // Start main application
    start_relayer().await
}
```

**4. Integration with Service Methods**

```rust
#[instrument(
    skip(self, request),
    fields(
        src_chain = %request.get_ref().src_chain,
        dst_chain = %request.get_ref().dst_chain,
    )
)]
async fn relay_by_tx(
    &self,
    request: Request<api::RelayByTxRequest>,
) -> Result<Response<api::RelayByTxResponse>, Status> {
    let inner_request = request.get_ref();
    let src_chain = inner_request.src_chain.clone();
    let dst_chain = inner_request.dst_chain.clone();

    // Wrap business logic with metrics tracking
    crate::metrics::track_metrics("relay_by_tx", &src_chain, &dst_chain, || async move {
        self.get_module(&inner_request.src_chain, &inner_request.dst_chain)?
            .relay_by_tx(request)
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Relay by tx request failed");
                Status::internal("Failed to relay by tx. See logs for more details.")
            })
    })
    .await
}
```

#### Custom Metrics Examples

**Business Logic Metrics:**
```rust
// Packet-specific metrics
static PACKETS_PROCESSED: LazyLock<IntCounterVec> = LazyLock::new(|| {
    register_int_counter_vec!(
        "eureka_relayer_packets_processed_total",
        "Packets processed by type",
        &["packet_type", "src_chain", "dst_chain", "status"]
    ).unwrap()
});

// Client expiry tracking
static CLIENT_EXPIRY_TIME: LazyLock<HistogramVec> = LazyLock::new(|| {
    register_histogram_vec!(
        "eureka_relayer_client_expiry_seconds",
        "Time until client expiry",
        &["client_id", "chain_id"],
        vec![3600.0, 7200.0, 86400.0, 604800.0]  // 1h, 2h, 1d, 1w buckets
    ).unwrap()
});
```

**Resource Monitoring:**
```rust
// Connection pool metrics
static RPC_CONNECTIONS: LazyLock<IntGauge> = LazyLock::new(|| {
    register_int_gauge!("eureka_relayer_rpc_connections_active", "Active RPC connections").unwrap()
});

// Queue depth tracking
static QUEUE_DEPTH: LazyLock<IntGaugeVec> = LazyLock::new(|| {
    register_int_gauge_vec!(
        "eureka_relayer_queue_depth",
        "Number of items in processing queues",
        &["queue_type"]
    ).unwrap()
});
```

#### Prometheus Configuration

**Scrape configuration for the local stack:**
```yaml
# In scripts/local-grafana-stack/prometheus.yml
scrape_configs:
  - job_name: 'ibc-eureka-relayer'
    static_configs:
      - targets: ['host.docker.internal:9000']
    scrape_interval: 15s
    metrics_path: /metrics
```

#### Best Practices for Rust

- **Static registration**: Use `LazyLock` for global metrics to avoid registration panics
- **Bounded cardinality**: Limit label values to known, finite sets
- **Middleware pattern**: Wrap service calls with `track_metrics()` for consistent RED metrics
- **Error categorization**: Group errors meaningfully, not by unique message
- **Resource monitoring**: Track connection pools, queues, and other finite resources
- **Background endpoint**: Run metrics HTTP server on separate port (9000) from main service

#### Common Patterns

**Conditional metrics:**
```rust
if critical_path {
    CRITICAL_OPERATIONS.inc();
}
```

**Timing operations:**
```rust
let timer = OPERATION_DURATION.with_label_values(&["fetch_data"]).start_timer();
let result = fetch_data().await;
drop(timer);  // Records duration automatically
```

**Gauge updates:**
```rust
// Track current state
QUEUE_DEPTH.with_label_values(&["pending_packets"]).set(queue.len() as i64);

// Track resource usage
RPC_CONNECTIONS.set(connection_pool.active_count() as i64);
```

#### Local Development with Observability Stack

The local Grafana stack includes Prometheus for metrics collection and visualization.

**Quick Setup:**

```bash
# Start the full observability stack
cd scripts/local-grafana-stack
docker compose up -d

# Enable metrics collection in e2e tests
export ENABLE_LOCAL_OBSERVABILITY=true

# Run tests to generate metrics
just e2e-test
```

**Accessing Metrics:**

- **Direct Prometheus endpoint**:
  - Relayer metrics: http://localhost:9000/metrics
  - Prometheus UI: http://localhost:9090

- **In Grafana**:
  1. Open http://localhost:3002
  2. Navigate to **Explore** → **Prometheus datasource**
  3. Query metrics: `eureka_relayer_request_total`
  4. Build dashboards with rate calculations

**Key Queries for Development:**

```promql
# Request rate (requests per second)
rate(eureka_relayer_request_total[5m])

# Error rate (percentage)
rate(eureka_relayer_response_codes{status_code!="0"}[5m]) / 
rate(eureka_relayer_response_codes[5m]) * 100

# Response time percentiles
histogram_quantile(0.95, rate(eureka_relayer_response_time_seconds_bucket[5m]))

# Active connections
eureka_relayer_connected_clients

# Operation breakdown by chain pair
sum by (src_chain, dst_chain) (rate(eureka_relayer_request_total[5m]))
```

**Development Tips:**

```bash
# Verify metrics endpoint is accessible
curl -s http://localhost:9000/metrics | grep eureka_relayer

# Generate load to see metrics in action
for i in {1..10}; do
  grpcurl -plaintext localhost:3001 ibc.applications.eureka.relayer.Relayer/Info
done

# Conditionally register metrics based on config
if config.enable_metrics {
    REQUEST_COUNTER.inc();
}
```

