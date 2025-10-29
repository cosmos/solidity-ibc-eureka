### BASE IMAGE ###
FROM debian:12-slim AS base

# Set the Agave and Rust versions
ARG AGAVE_VERSION=2.3.7
ARG RUST_VERSION=stable
ARG USER=solana
ARG WORKSPACE=/workspace
ARG WORKSPACE_BIN=$WORKSPACE/bin
ARG WORKSPACE_SRC=$WORKSPACE/src

# Set the working directory
WORKDIR $WORKSPACE

# Base OS dependencies
RUN apt update && \
    apt-get install -y bzip2 ca-certificates tini && \
    rm -rf /var/lib/apt/lists/*

# Use tini as the entry point
ENTRYPOINT ["/usr/bin/tini", "--"]

### BUILDER IMAGE ###
FROM base AS builder

# Install OS dependencies
RUN apt update && \
    apt-get install -y build-essential clang cmake curl libclang-dev llvm-dev libudev-dev pkg-config protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*

# Setup Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain $RUST_VERSION -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Get the Agave source
RUN curl https://codeload.github.com/anza-xyz/agave/tar.gz/refs/tags/v$AGAVE_VERSION | tar xvz
RUN mv $WORKSPACE/agave-$AGAVE_VERSION $WORKSPACE_SRC

# Move to the src directory
WORKDIR $WORKSPACE_SRC

# Create the bin directory
RUN mkdir -pv "$WORKSPACE_BIN"

# Build the solana binary and copy it to /workspace/bin
RUN cargo build --bin solana --release && cp $WORKSPACE_SRC/target/release/solana $WORKSPACE_BIN/solana

# Build the solana-test-validator binary and copy it to /workspace/bin
RUN cargo build --bin solana-test-validator --release && cp $WORKSPACE_SRC/target/release/solana-test-validator $WORKSPACE_BIN/solana-test-validator

### FINAL IMAGE ###
FROM base AS final

# Copy the binary from the builder image
COPY --from=builder $WORKSPACE_BIN/* $WORKSPACE_BIN/

# Create a non-root user
RUN groupadd --gid 1000 $USER && \
    useradd --uid 1000 --gid $USER --create-home --shell /bin/bash $USER && \
    mkdir -p $WORKSPACE_BIN && chown -R $USER:$USER $WORKSPACE_BIN

# Set ownership for the non-root user
RUN chown -R $USER:$USER $WORKSPACE

# Switch to the non-root user
USER $USER

# Add the bin directory to the PATH
ENV PATH="$WORKSPACE_BIN:${PATH}"

# Expose the Solana Test Validator ports
EXPOSE 8899 8900

# Run the solana-test-validator by default
CMD ["solana-test-validator"]
