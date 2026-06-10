# ── Stage 1: install cargo-chef ──────────────────────────────────────────────
FROM rust:1.96-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

# ── Stage 2: compute the dependency recipe ───────────────────────────────────
# Copies the full source tree so cargo-chef can walk all workspace Cargo.toml
# files and produce a recipe that captures the exact dependency graph.
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ── Stage 3: build dependencies, then the binaries ───────────────────────────
FROM chef AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    protobuf-compiler \
    libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

# Compile only dependencies (layer is cached until Cargo.toml / Cargo.lock changes)
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

# Now copy source and build both binaries in one invocation (shared dep compilation)
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi
COPY proto proto
RUN cargo build --bin proof-api --bin operator --release --locked

# ── Stage 4: CA certificates ─────────────────────────────────────────────────
FROM debian:bookworm-slim AS certs
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# ── Stage 5: minimal runtime image ───────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:debug

ARG BUILD_DATE
ARG VCS_REF
ARG VERSION
ARG COMMIT
LABEL org.opencontainers.image.title="proof-api" \
      org.opencontainers.image.description="Solidity IBC Eureka Proof API" \
      org.opencontainers.image.source="https://github.com/cosmos/solidity-ibc-eureka" \
      org.opencontainers.image.revision=$VCS_REF \
      org.opencontainers.image.created=$BUILD_DATE \
      org.opencontainers.image.version=$VERSION

COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

COPY --from=builder /app/target/release/proof-api /usr/local/bin/proof-api
COPY --from=builder /app/target/release/operator /usr/local/bin/operator
COPY scripts/proof_api_docker_entrypoint.sh /docker_entrypoint.sh

ENV PLATFORM_COMMIT=${COMMIT}
ENTRYPOINT ["sh", "/docker_entrypoint.sh", "start", "--config", "/usr/local/proof-api/proof-api.json"]
