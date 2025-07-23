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
```

## Run Locally

1. Run the attestor

    a. Run: `cargo run -- key generate` to generate a private key

    b. Run `cargo run -- server solana --config server.dev.toml` to start a dev server

2. Run the aggregator

    a. Run `cargo run -- server --config config.example.toml`

    b. Query `grpcurl -plaintext -d '{"min_height": 394277673}' localhost:8080 aggregator.Aggregator.GetAggregateAttestation | jq`

## Docker Setup

This directory contains a complete Docker Compose setup for running 3 IBC attestor instances and 1 sig-aggregator locally.

### Quick Start

From the workspace root:

```sh
# Start all three attestor and one aggregator
just start-aggregator-services

# Test the services
just test-aggregator-services

# Stop services (cleans up volumes too)
just stop-aggregator-services
```

Or manually from this directory:

```sh
# Start services - the --wait flag ensures all health checks pass before returning
docker-compose down --volumes && docker-compose up --build -d --wait

# Stop services (with volume cleanup)
docker-compose down --volumes
```

The setup includes:

- **3 IBC Attestor instances** on ports 9000, 9001, 9002
- **1 Sig-Aggregator** on port 8080 (requires 2/3 quorum)

Configuration files are in the `config/` directory.
