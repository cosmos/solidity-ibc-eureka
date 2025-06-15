#!/bin/bash

if [ -z "$1" ]; then
  echo "Error: SP1_PROGRAM_VERSION argument is required."
  exit 1
fi

SP1_PROGRAM_VERSION="$1"

if [ -z "$NETWORK_PRIVATE_KEY" ]; then
  echo "Error: NETWORK_PRIVATE_KEY environment variable is required."
  exit 1
fi

docker run -p 3000:3000 -e RUST_LOG=debug -e RUST_BACKTRACE=1 -e RUST_LIB_BACKTRACE=1 -e SP1_PROGRAM_VERSIONS="$SP1_PROGRAM_VERSION" -e NETWORK_PRIVATE_KEY="$NETWORK_PRIVATE_KEY" -v ./scripts/relayer-volume:/usr/local/relayer relayer:local
