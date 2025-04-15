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

RUN cargo build --bin relayer --release --locked
RUN cargo build --bin operator --release --locked

COPY scripts/relayer_docker_entrypoint.sh /docker_entrypoint.sh

FROM gcr.io/distroless/cc-debian12:debug
WORKDIR /usr/local/bin
COPY --from=build /etc/ssl/certs /etc/ssl/certs
COPY --from=build /solidity-ibc-eureka/target/release/relayer /usr/local/bin/relayer
COPY --from=build /solidity-ibc-eureka/target/release/operator /usr/local/bin/operator
COPY --from=build /docker_entrypoint.sh /docker_entrypoint.sh

ENTRYPOINT ["sh", "/docker_entrypoint.sh", "start", "--config", "/usr/local/relayer/relayer.json"]
