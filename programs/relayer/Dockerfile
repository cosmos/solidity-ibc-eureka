FROM rust:1.85-bookworm AS build

RUN apt update && apt install -y build-essential protobuf-compiler
RUN curl -L https://sp1.succinct.xyz | bash

ENV PATH="/root/.cargo/bin:${PATH}"
ENV PATH="/root/.sp1/bin:${PATH}"

WORKDIR /solidity-ibc-eureka/
COPY Cargo.toml Cargo.lock ./
COPY packages packages
COPY programs programs
COPY contracts contracts
COPY abi abi

RUN sp1up
