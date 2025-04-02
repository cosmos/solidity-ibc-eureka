FROM rust:1.85-bookworm AS build

RUN apt update && apt install -y build-essential protobuf-compiler ca-certificates

ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /solidity-ibc-eureka/
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi

RUN cargo build --bin relayer --release --locked
RUN cargo build --bin operator --release --locked

FROM gcr.io/distroless/cc-debian12:latest
WORKDIR /usr/local/bin
COPY --from=build /etc/ssl/certs /etc/ssl/certs
COPY --from=build /solidity-ibc-eureka/target/release/relayer /usr/local/bin/relayer
COPY --from=build /solidity-ibc-eureka/target/release/operator /usr/local/bin/operator
ENTRYPOINT [ "/usr/local/bin/relayer", "start", "--config", "/usr/local/relayer/relayer.json"]
