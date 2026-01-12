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

RUN cargo build --release --locked --bin relayer

FROM gcr.io/distroless/cc-debian12:debug

WORKDIR /usr/local/bin

COPY scripts/docker/all-in-one-entrypoint.sh /entrypoint.sh

COPY --from=build /etc/ssl/certs /etc/ssl/certs

COPY --from=build /src/target/release/relayer /usr/local/bin/relayer

# 3000 is the relayer port
# 9000 is the relayer metrics port
# 8081 is the relayer grpc web port
EXPOSE 3000 9000 8081

ENTRYPOINT [ "sh", "/entrypoint.sh" ]