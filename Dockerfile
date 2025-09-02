# v0.1 Dockerfile (corrected for TLS/SSL)
# NOTE: Still no "Rust x Docker" optimizations (cargo-chef, sccache, etc.)

FROM rust:1.89-bookworm AS build

ARG CARGO_TERM_COLOR=always
ARG RUSTFLAGS="-C codegen-units=8"

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential protobuf-compiler pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Build binaries
RUN cargo build --release --locked --bin relayer && \
    cargo build --release --locked --bin ibc_attestor -F op && \
    mv target/release/ibc_attestor target/release/attestor-optimism && \
    cargo build --release --locked --bin ibc_attestor -F arbitrum && \
    mv target/release/ibc_attestor target/release/attestor-arbitrum && \
    cargo build --release --locked --bin ibc_attestor -F cosmos && \
    mv target/release/ibc_attestor target/release/attestor-cosmos


FROM debian:bookworm-slim

# Runtime deps:
# - ca-certificates: for TLS root store
# - libssl3: for native-tls/openssl-sys dynamic linking (pulls libcrypto3)
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 && \
    update-ca-certificates && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /usr/local/bin

COPY scripts/docker/all-in-one-entrypoint.sh /entrypoint.sh

COPY --from=build /src/target/release/relayer /usr/local/bin/relayer
COPY --from=build /src/target/release/attestor-optimism /usr/local/bin/attestor-optimism
COPY --from=build /src/target/release/attestor-arbitrum /usr/local/bin/attestor-arbitrum
COPY --from=build /src/target/release/attestor-cosmos /usr/local/bin/attestor-cosmos

# 3000 - relayer
# 9000 - relayer metrics
# 8080 - attestor
EXPOSE 3000 9000 8080

ENTRYPOINT ["sh", "/entrypoint.sh"]
