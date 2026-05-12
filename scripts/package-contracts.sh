#!/usr/bin/env bash
#
# Build and package pre-compiled solidity contract artifacts as a release tarball.
#
# Performs a clean build (bun install + forge build), then copies the full
# forge JSON (ABI + bytecode + metadata) for each shipped contract into
# release-artifacts/solidity-contracts/bytecode/, stamps a VERSION file and
# LICENSE, and produces solidity-contracts-<version>.tar.gz at the repo root.
#
# Requires `bun` and `forge` on PATH.
#
# Usage:   ./scripts/package-contracts.sh [version]
# Example: ./scripts/package-contracts.sh solidity-v2.0.1

set -euo pipefail

VERSION="${1:-dev}"

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$REPO_ROOT"

# Contracts to ship as full forge JSON (ABI + bytecode + metadata)
contracts=(
  ICS26Router
  ICS20Transfer
  ICS27Account
  ICS27GMP
  SP1ICS07Tendermint
  AttestationLightClient
  ERC20
  IBCERC20
  RelayerHelper
  TestIFT
)

echo "🧹 Cleaning previous build output"
rm -rf cache out broadcast

echo "📦 Installing contract dependencies"
bun install --frozen-lockfile

echo "🔨 Building contracts"
forge build

staging="release-artifacts/solidity-contracts"
rm -rf "$staging"
mkdir -p "$staging/bytecode"

for c in "${contracts[@]}"; do
  src="out/${c}.sol/${c}.json"
  if [ ! -f "$src" ]; then
    echo "❌ Missing forge artifact: $src"
    exit 1
  fi
  cp "$src" "$staging/bytecode/${c}.json"
done

cp LICENSE.md "$staging/"
echo "$VERSION" > "$staging/VERSION"

tarball="solidity-contracts-${VERSION}.tar.gz"
tar -czvf "$tarball" -C release-artifacts solidity-contracts

echo "✅ Packaged: $tarball"
