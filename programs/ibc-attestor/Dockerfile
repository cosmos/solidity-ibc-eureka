FROM rust:1.88-alpine AS build

RUN apk add --no-cache \
    musl-dev \
    protobuf-dev \
    openssl-dev \
    pkgconfig

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi
COPY proto proto

RUN cargo build --bin ibc_attestor --release --locked

FROM alpine:latest

RUN apk add --no-cache \
    bash \
    ca-certificates \
    libgcc

WORKDIR /usr/local/bin
COPY --from=build /app/target/release/ibc_attestor /usr/local/bin/ibc_attestor

ENTRYPOINT ["ibc_attestor"]
