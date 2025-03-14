#!/usr/bin/env bash
set -e

# Find all tests
cd e2e/interchaintestv8
TESTS=$(grep -R "func (s \*.*Suite) Test" . | sed -E 's/.*(Test[[:alnum:]_]+)\(.*/\1/')

# Convert to a JSON array
JSON_ARRAY=$(echo "$TESTS" | jq -R . | jq -s .)

# Filter out skipped tests if a skip pattern was provided
SKIP_TESTS="$1"
if [ -n "$SKIP_TESTS" ]; then
  JSON_ARRAY=$(echo "$JSON_ARRAY" | jq --arg skip "$SKIP_TESTS" '
    def skip_array: ($skip | split(","));
    map(select(. as $t | skip_array | index($t) | not))
  ')
fi

# Create and output the matrix object
jq -nc --argjson arr "$JSON_ARRAY" '{ test: $arr }'
