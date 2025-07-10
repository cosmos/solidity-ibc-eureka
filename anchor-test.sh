#!/bin/bash

# Simple script to run anchor test from root directory
# This automatically copies .so files to the correct location and deploys

# Directory for the Solana Anchor program
SOLANA_DIR="programs/solana"

# Copy all files from target/deploy to programs/solana/target/deploy if any exist
if compgen -G "target/deploy/*" > /dev/null; then
    mkdir -p "$SOLANA_DIR/target/deploy"
    cp -f target/deploy/* "$SOLANA_DIR/target/deploy/"
    echo "✅ Copied all files from target/deploy to $SOLANA_DIR/target/deploy/ (overwriting if needed)"
fi

# Run anchor test with all arguments passed through
# Prefer anchor-nix if available, otherwise fallback to anchor
if command -v anchor-nix >/dev/null 2>&1; then
    echo "🦀 Using anchor-nix"
    (cd "$SOLANA_DIR" && anchor-nix test "$@")
else
    echo "🦀 Using anchor"
    (cd "$SOLANA_DIR" && anchor test "$@")
fi
