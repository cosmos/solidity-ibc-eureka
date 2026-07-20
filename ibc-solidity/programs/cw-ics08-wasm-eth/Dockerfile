# syntax=docker/dockerfile:1.7
# Tool-only image: contains toolchain + wasm-opt; builds/optimizes when run.
FROM rust:1.86-slim AS tool

ARG DEBIAN_FRONTEND=noninteractive
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_HOME=/usr/local/rustup \
    RUST_BACKTRACE=1

# Minimal but safe set of native dependencies commonly needed by CosmWasm crates.
# - binaryen -> wasm-opt
# - clang/libclang-dev -> bindgen use across some deps
# - protobuf-compiler -> in case build scripts generate protos
# - build-essential/pkg-config/libssl-dev -> common native deps
# - git/cmake/ca-certificates -> common tooling
RUN apt-get update && apt-get install -y --no-install-recommends \
      binaryen \
      clang \
      libclang-dev \
      protobuf-compiler \
      build-essential \
      pkg-config \
      libssl-dev \
      git \
      cmake \
      ca-certificates \
  && rm -rf /var/lib/apt/lists/*

# Add wasm target
RUN rustup target add wasm32-unknown-unknown

WORKDIR /code

# Tiny entrypoint that builds and optimizes the specified package.
# Defaults to cw-ics08-wasm-eth@1.3.0 and writes artifacts/ on the mounted repo.
RUN set -eux; \
  cat >/usr/local/bin/build-wasm.sh <<'EOF' \
  && chmod +x /usr/local/bin/build-wasm.sh
#!/bin/sh
set -eu

PACKAGE="${PACKAGE:-cw-ics08-wasm-eth@1.3.0}"
PROJECTDIR="${1:-/code}"
TARGET_DIR="${TARGET_DIR:-./target}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-./artifacts}"
LOCKED_FLAG="${LOCKED_FLAG:---locked}"
WASM_OPT_FLAGS="${WASM_OPT_FLAGS:--Os}"

echo "==> Building $PACKAGE"
echo "    project:   $PROJECTDIR"
echo "    target:    $TARGET_DIR"
echo "    artifacts: $ARTIFACTS_DIR"

cd "$PROJECTDIR"
mkdir -p "$ARTIFACTS_DIR"

# Clean only existing wasm outputs; keep the target cache warm.
rm -f "$TARGET_DIR/wasm32-unknown-unknown/release/"*.wasm 2>/dev/null || true

cargo build --release --lib \
  --target-dir "$TARGET_DIR" \
  --target wasm32-unknown-unknown \
  $LOCKED_FLAG \
  -p "$PACKAGE"

found=0
for WASM in "$TARGET_DIR/wasm32-unknown-unknown/release/"*.wasm; do
  [ -e "$WASM" ] || continue
  found=1
  out="$(basename "$WASM")"
  echo "==> Optimizing $out with wasm-opt $WASM_OPT_FLAGS"
  wasm-opt $WASM_OPT_FLAGS "$WASM" -o "$ARTIFACTS_DIR/$out"
done

if [ "$found" -eq 0 ]; then
  echo "Error: no .wasm built in $TARGET_DIR/wasm32-unknown-unknown/release/"
  exit 1
fi

( cd "$ARTIFACTS_DIR" && sha256sum -- *.wasm | tee checksums.txt )
echo "==> Done. Artifacts in: $ARTIFACTS_DIR"
EOF

ENTRYPOINT ["/usr/local/bin/build-wasm.sh"]
# Default argument: path to the mounted workspace
CMD ["/code"]
