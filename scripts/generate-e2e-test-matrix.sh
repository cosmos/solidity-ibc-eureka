#!/usr/bin/env bash
set -e

# Find all tests
cd e2e/interchaintestv8
TESTS=$(grep -R "func (s \*.*Suite) Test" . | while read -r line; do
  # Extract file name and suite name
  file_name=$(echo "$line" | cut -d':' -f1)
  suite_name=$(echo "$line" | sed -E 's/.*func \(s \*([[:alnum:]_]+)\).*/\1/')
  test_name=$(echo "$line" | sed -E 's/.*(Test[[:alnum:]_]+)\(.*/\1/')
  
  # Find the top-level test function for this suite
  top_level_test=$(grep -A1 -B1 "$suite_name" "$file_name" | grep "func Test" | head -1 | sed -E 's/.*func (Test[[:alnum:]_]+).*/\1/')
  
  # Output the combined test name
  if [ -n "$top_level_test" ]; then
    echo "$top_level_test/$test_name"
  else
    echo "$test_name"
  fi
done)

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
