# Tracing

Tracing is a framework for instrumenting Rust programs to collect structured, event-based diagnostic information.

Subscribers do the heavy lifting.

Subscriber should be global Static. But on rare occasion, we can have case specific subscriber. Most common is std out with combination with Otel compatible subscriber.

Every sisngle trace has to have a unique identifier to it. TraceID (16 bytes Otel) or CCID (Corealtion Context Identifier)

Span = smallest unit in a trace.
Span = a period of time in the program.

Inject = Taking the current span context, serializing it and sending it to the child call.
Extract = Discover the span context, and create a child span using that data.
Context can add up. Be mindful of it.

A well-traced microservice should attach relevant semantic attributes (such as the OpenTelemetry span.kind attribute) to the spans that it generates.

Distributed request tracing works best when the entire traced operation takes place in a fairly short (minutes) time span.

- data retention periods for trace analyzers and sampling considerations
If you are trying to trace operations with an extremely long execution time, don’t fret,
there are options to address those use cases.

Open telemetry API provides three major things:

- Distributed context propagation
- Application tracing
- Application metrics

Span Context:

- TraceID: 16 byte array.
- SpanID: 8 byte array.
- TraceFlags: Detail about the trace
- IsRemote: A boolean flag shown if the context was propagated from a remote parent.

They help you build a relational graph of sorts, showing you what happened
(through names) and why it happened (through tags). Logs could be thought of as the
how it happened piece of this puzzle

```md
Span name: What happened
Span tags: Why happened
Span Logs: How happened
```

If every service emits one span with some basic attributes that require no run‐
time overhead (i.e., string values that can be precalculated at service initialization)
then the total added overhead to each request is simply the propagation of trace con‐
text headers, a task that adds 25 bytes on the wire and a negligible amount of cycles to
decode afterwards.

## Instrument Checklist

- Make sure all the spans are created are also finished, even if unrecoverable errors. [if possible]
- Egress and ingress spans have SpanKind set.
- Spans should include identifying underlying infra
  - Hostname / Applicatioin instance
  - App server version
  - Region/Availability zone
- Attributes are namespaced
- **Attrbutes with numeric values should include the unit of measurement in the key name. Ex: `payload_size_kb` is good `payload_size` is bad.**
- Version attributes are extremely important

TODO: In rust instrument, can I make some variables show or skip based on if there were any error occurred or not?

Every time you trace, you have to keep in mind what unknown-unknown I need to know and how this tracing would affct the performance.

## Rust

Necessary carets

- tracing – to instrument our Rust code.
- tracing-subscriber – allows us to listen for tracing events and define how they are filtered and exported.
- tracing-log - Allowes us to collect all log's event to our subscriber. This is useful when we want to collect 3rd party logs. `LogTracer::init().expect("Failed to set logger");`
- opentelemetry – OpenTelemetry’s API-level view of tracing, spans, etc.
- opentelemetry_sdk – Implements the OpenTelemetry APIs 2.
- tracing-opentelemetry – provides a compatibility layer between the two.
- opentelemetry-otlp – the protocol implementation to export data to Jaeger or some other backend.

Any trace events generated outside the context of a subscriber will not be collected.
Instrumentation only works if a subscriber is set.

**Libraries should NOT install a subscriber by using a method that calls set_global_default(), as this will cause conflicts when executables try to set the default later.**

We can change the tracing filters (during runtime) [https://docs.rs/tracing-subscriber/latest/tracing_subscriber/reload/index.html]

### Span

A span is a cheap handle (ID + reference to the current subscriber). Conceptually, it's a key to subscriber's storage.
Creation process:

1. macro build a static metadata
2. Calls `Subscriber::new_span(&Attributes)
3. Layers called `on_new_span` with the Attributes
4. A `tracing::Span` handler is returned, which holds that `Id` and cloned `Dispatch`

**A span does not become cueent until it's entered.**

We can manually create `span`

```rust
pub async fn subscribe(/* */) -> HttpResponse {
    let request_id = Uuid::new_v4();
    let request_span = tracing::info_span!(
        "Adding a new subscriber.",
        %request_id,
        subscriber_email = %form.email,
        subscriber_name = %form.name
    );
    // Using `enter` in an async function is a recipe for disaster!
    let _guard = request_span.enter();
}
```

`span.enter()` pushes it on top of the current span stack.
The stack of entered spans are thread local. (Or task local if you use `#[instrument]`)

Spans are itself **not thread local**. `tracing::Span` is `Send + Sync`.
But the `current_span` is thread local.

Manually entering a span in a future does not gurantee

You can enter or exit a span multiple times. Once you close, that's final.

### Subscriber

`tracing::Subscriber` is the trait. Implemented by the `Registry`
`tracing-subscriber` has some basic implementations.

`tokio-console` is a subscriber used for debugging and profiling for async rust applicactions. But this requires tokio-unstable enabled.

### Registry

- Does not record the traces itself.
- Stores span metadata
- Record relationship between spans
- Track which spans are active and which are closed.

### Best Practices

Always use line number

Never leak credentials.

Libraries should only rely on the tracing crate and use the provided macros and types to collect whatever information might be useful to downstream consumers.Ref: [https://github.com/tokio-rs/tracing]

```rust
#[instrument(skip(self, credential), fields(username = %credential.username))]
```

Use `secrecy::Secret` crate to mask your secrets (pw, key etc)

Understand how to use `tracing::field::Empty`

Using tracing-error with eyre can produce rich, understandable errors.

```rust
tracing_subscriber::registry()
    .with(filter_layer)
    .with(fmt_layer)
    .with(tracing_error::ErrorSubscriber::default())
    .init();

color_eyre::install()?;
```

If you want human readable well formatted traces use `.pretty()`
`fmt::layer().pretty()`

### Antipatterns

When in doubt, make a new span rather than a new logging statement. [Put reference]

If unsure, skip self from instrumentation.

Turn off time from tracing if not in production.

When in doubt, make a new span rather than a new logging statement.

If we have a failable function we can print out the error automaticaly with a tracing setting
This is anti the idea that error should be handled once.

```rust
#[instrument(level = "trace", err)]
```

Use color-eyre crate: [https://docs.rs/color-eyre/0.6.5/color_eyre/]

#### Simple tracing library

```rust
pub fn get_subscriber(
    name: String,
    env_filter: String
    sink: Sink,
) -> impl Subscriber + Send + Sync
    where
        Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,
{
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(env_filter));
    let formatting_layer = BunyanFormattingLayer::new(name, sink); // Maybe Otel
    Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
}
```

```rust
pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    LogTracer::init().expect("Failed to set logger");
    set_global_default(subscriber).expect("Failed to set subscriber");
}
```

```rust
#[tokio::main]
async fn main() -> std::io::Result<()> {
    let subscriber = get_subscriber("name".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);
}
```

For testing

```rust
use once_cell::sync::Lazy;

// Ensure that the `tracing` stack is only initialised once using `once_cell`
static TRACING: Lazy<()> = Lazy::new(|| {
    let default_filter_level = "info".to_string();
    let subscriber_name = "test".to_string();

    if std::env::var("MUTE_TEST_LOG").is_ok() {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::sink);
        init_subscriber(subscriber);

    } else {
        let subscriber = get_subscriber(subscriber_name, default_filter_level, std::io::stdout);
        init_subscriber(subscriber);
    };
});

pub struct TestApp {
    pub address: String,
    pub db_pool: PgPool,
}

async fn test_app() -> TestApp {
    // The first time `initialize` is invoked the code in `TRACING` is executed.
    // All other invocations will instead skip execution.
    Lazy::force(&TRACING);
}
```
