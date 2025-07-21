FROM rust:1.85-bookworm AS build

RUN apt update && apt install -y build-essential protobuf-compiler ca-certificates

ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /solidity-ibc-eureka/
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi
COPY proto proto

RUN cargo build --bin aggregator --release --locked

FROM gcr.io/distroless/cc-debian12:debug
WORKDIR /usr/local/bin
COPY --from=build /etc/ssl/certs /etc/ssl/certs
COPY --from=build /solidity-ibc-eureka/target/release/aggregator /usr/local/bin/aggregator

ENTRYPOINT ["aggregator"]