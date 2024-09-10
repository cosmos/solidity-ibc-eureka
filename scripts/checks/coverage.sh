#!/usr/bin/env bash

set -euo pipefail

# Foundry coverage
forge coverage --report lcov
# Remove zero hits
sed -i '/,0/d' lcov.info

# Reports are then uploaded to Codecov automatically by workflow, and merged.
