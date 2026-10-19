FROM rust:1.92-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    protobuf-compiler \
    libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY ibc-solidity/contracts ibc-solidity/contracts
COPY ibc-solidity/abi ibc-solidity/abi
COPY ibc-solidity/programs ibc-solidity/programs
# `ibc-solana` is a separate workspace; its `packages/*` are path dependencies of
# `packages/tendermint-light-client/solana`. The workspace manifest is needed so those
# packages can resolve their inherited (`workspace = true`) fields.
COPY ibc-solana/Cargo.toml ibc-solana/Cargo.toml
COPY ibc-solana/packages ibc-solana/packages
COPY proto proto

RUN cargo build --bin proof-api --release --locked

FROM debian:bookworm-slim AS certs
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

FROM gcr.io/distroless/cc-debian12:debug

# OCI labels (source, revision, version, ...) are set by docker/metadata-action in
# .github/workflows/docker.yml so they follow the repository without hard-coding it.
ARG COMMIT

COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

COPY --from=builder /app/target/release/proof-api /usr/local/bin/proof-api
COPY scripts/proof_api_docker_entrypoint.sh /docker_entrypoint.sh

ENV PLATFORM_COMMIT=${COMMIT}
ENTRYPOINT ["sh", "/docker_entrypoint.sh", "start", "--config", "/usr/local/proof-api/proof-api.json"]