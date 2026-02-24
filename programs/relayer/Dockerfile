FROM rust:1.91-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi
COPY proto proto

RUN cargo build --bin relayer --release --locked
RUN cargo build --bin operator --release --locked

FROM debian:bookworm-slim AS certs
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

FROM gcr.io/distroless/cc-debian12:debug

ARG BUILD_DATE
ARG VCS_REF
ARG VERSION
ARG COMMIT
LABEL org.opencontainers.image.title="solidity-ibc-eureka-relayer" \
      org.opencontainers.image.description="Solidity IBC Eureka Relayer" \
      org.opencontainers.image.source="https://github.com/cosmos/solidity-ibc-eureka" \
      org.opencontainers.image.revision=$VCS_REF \
      org.opencontainers.image.created=$BUILD_DATE \
      org.opencontainers.image.version=$VERSION

COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

COPY --from=builder /app/target/release/relayer /usr/local/bin/relayer
COPY --from=builder /app/target/release/operator /usr/local/bin/operator
COPY scripts/relayer_docker_entrypoint.sh /docker_entrypoint.sh

ENV PLATFORM_COMMIT=${COMMIT}
ENTRYPOINT ["sh", "/docker_entrypoint.sh", "start", "--config", "/usr/local/relayer/relayer.json"]
