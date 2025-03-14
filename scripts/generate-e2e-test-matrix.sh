#!/usr/bin/env bash

set -e

TEST_DIR="e2e/interchaintestv8"
echo "Looking for tests in: $TEST_DIR"

if [ ! -d "$TEST_DIR" ]; then
  echo "Error: Test directory '$TEST_DIR' not found"
  exit 1
fi

cd "$TEST_DIR"

# Find all tests
echo "Finding all tests..."
TESTS=$(grep -R "func (s \*.*Suite) Test" . | sed -E 's/.*(Test[[:alnum:]_]+)\(.*/\1/')

TEST_COUNT=$(echo "$TESTS" | grep -v "^$" | wc -l)
if [ "$TEST_COUNT" -eq 0 ]; then
  echo "Error: No tests found in $TEST_DIR"
  exit 1
fi

# Conver to a JSON array
JSON_ARRAY=$(echo "$TESTS" | jq -R . | jq -s .)
# Then to a complete matrix object
MATRIX=$(jq -n --argjson arr "$JSON_ARRAY" '{ test: $arr }')

# Print the matrix JSON
echo "$MATRIX"
