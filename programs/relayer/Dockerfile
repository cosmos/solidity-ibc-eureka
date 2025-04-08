FROM ubuntu:24.04 AS build

RUN apt update && apt install -y build-essential protobuf-compiler curl git
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | bash -s -- -y
RUN curl -L https://sp1.succinct.xyz | bash

ENV PATH="/root/.cargo/bin:${PATH}"
ENV PATH="/root/.sp1/bin:${PATH}"

RUN sp1up

WORKDIR /solidity-ibc-eureka/
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi
COPY proto proto

RUN cargo build --bin relayer --release --locked
RUN cargo build --bin operator --release --locked

FROM ubuntu:24.04
WORKDIR /usr/local/bin
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=build /solidity-ibc-eureka/target/release/relayer /usr/local/bin/relayer
COPY --from=build /solidity-ibc-eureka/target/release/operator /usr/local/bin/operator
ENTRYPOINT [ "/usr/local/bin/relayer", "start", "--config", "/usr/local/relayer/relayer.json"]
