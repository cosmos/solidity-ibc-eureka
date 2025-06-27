# Aggregator Service

This project implements a gRPC-based service for aggregating attestations from multiple sources.

It consists of three main components:

1. **Attestor**: A mock service that provides attestations for given block heights.
2. **Aggregator**: The main service that queries multiple attestors, handles failures, and finds the highest block height with a quorum of signatures.
3. **Relayer**: A simple client that queries the aggregator.

## Building the Project

To build the project, you need to have the Rust toolchain and `protoc` (the Protocol Buffers compiler) installed.

```sh
cargo build --release
