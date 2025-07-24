FROM rust:1.88-bullseye AS build

RUN apt-get update && apt-get install -y \
    protobuf-compiler

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY proto proto

RUN cargo build --bin ibc_attestor --release --locked

FROM debian:bullseye-slim

ARG PORT

RUN apt-get update && apt-get install -y \
    ca-certificates

WORKDIR /usr/local/bin
COPY --from=build /app/target/release/ibc_attestor /usr/local/bin/ibc_attestor
COPY test/sig-aggregator/configs/attestor.toml /tmp/attestor.toml

RUN sed -i "s/port = 9000/port = ${PORT}/g" /tmp/attestor.toml

ENTRYPOINT ["ibc_attestor"]
