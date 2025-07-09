#!/bin/bash

# Simple script to run anchor test from root directory
# This automatically copies .so files to the correct location and deploys

cd programs/solana

# Copy .so file if it exists in root workspace
if [ -f "../../target/deploy/ics07_tendermint.so" ]; then
    mkdir -p target/deploy
    cp "../../target/deploy/ics07_tendermint.so" "target/deploy/"
    echo "✅ Copied ics07_tendermint.so to target/deploy/"
fi

# Copy keypair if it exists in root workspace  
if [ -f "../../target/deploy/ics07_tendermint-keypair.json" ]; then
    mkdir -p target/deploy
    cp "../../target/deploy/ics07_tendermint-keypair.json" "target/deploy/"
    echo "✅ Copied ics07_tendermint-keypair.json to target/deploy/"
fi

# Run anchor test with all arguments passed through
# Note: anchor test automatically starts validator and deploys programs
anchor test "$@"
