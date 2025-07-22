FROM rust:1.88-bookworm AS build

RUN apt update && apt install -y build-essential protobuf-compiler ca-certificates

ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /solidity-ibc-eureka/
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi
COPY proto proto

RUN cargo build --bin ibc_attestor --release --locked

# Download and install grpcurl for health checks
RUN apt-get update && apt-get install -y curl \
    && curl -sSL "https://github.com/fullstorydev/grpcurl/releases/download/v1.8.7/grpcurl_1.8.7_linux_x86_64.tar.gz" \
    | tar -xz -C /usr/local/bin

FROM debian:bookworm-slim

# Install required runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    bash \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/local/bin
COPY --from=build /etc/ssl/certs /etc/ssl/certs
COPY --from=build /solidity-ibc-eureka/target/release/ibc_attestor /usr/local/bin/ibc_attestor
COPY --from=build /usr/local/bin/grpcurl /usr/local/bin/grpcurl

ENTRYPOINT ["ibc_attestor"]
