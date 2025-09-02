# Note that NONE of "rust x docker" optimizations are applied to this Dockerfile
# Because the binaries will be converged & simplified
# Consider this as v0.1 Dockerfile
# TODO: cargo chef
# TODO: sccache

FROM rust:1.89-bookworm AS build

ARG CARGO_TERM_COLOR=always
ARG RUSTFLAGS="-C codegen-units=8"

RUN apt update && apt install -y build-essential protobuf-compiler ca-certificates

WORKDIR /src

COPY . .

RUN cargo build --release --locked --bin relayer && \
    cargo build --release --locked --bin ibc_attestor -F op && \
    mv target/release/ibc_attestor target/release/attestor-optimism && \
    cargo build --release --locked --bin ibc_attestor -F arbitrum && \
    mv target/release/ibc_attestor target/release/attestor-arbitrum && \
    cargo build --release --locked --bin ibc_attestor -F cosmos && \
    mv target/release/ibc_attestor target/release/attestor-cosmos

FROM debian:bookworm-slim

WORKDIR /usr/local/bin

COPY scripts/docker/all-in-one-entrypoint.sh /entrypoint.sh

COPY --from=build /etc/ssl/certs /etc/ssl/certs

COPY --from=build /src/target/release/relayer /usr/local/bin/relayer

COPY --from=build /src/target/release/attestor-optimism /usr/local/bin/attestor-optimism
COPY --from=build /src/target/release/attestor-arbitrum /usr/local/bin/attestor-arbitrum
COPY --from=build /src/target/release/attestor-cosmos /usr/local/bin/attestor-cosmos

# 3000 is the relayer port
# 9000 is the relayer metrics port
# 8080 is the attestor port
EXPOSE 3000 9000 8080

ENTRYPOINT [ "sh", "/entrypoint.sh" ]