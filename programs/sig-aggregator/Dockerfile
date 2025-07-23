FROM rust:1.88-bullseye AS build

RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi
COPY proto proto

RUN cargo build --bin aggregator --release --locked

FROM debian:bullseye-slim

ARG PORT

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/local/bin
COPY --from=build /app/target/release/aggregator /usr/local/bin/aggregator
COPY test/sig-aggregator/configs/aggregator.toml /tmp/aggregator.toml
ENTRYPOINT ["aggregator"]
