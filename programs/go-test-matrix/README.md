# go-test-matrix

This tool scans Go test files to discover test functions and their associated testify test suites, then generates a JSON matrix suitable for GitHub Actions workflows.

## Usage

```bash
# Generate matrix for all tests
go run main.go

# Filter to specific test name (returns that test from suites that have it, plus all tests from other suites)
TEST_NAME=Test_Deploy go run main.go

# Filter to specific test suite (returns only tests from that suite)
TEST_ENTRYPOINT=TestWithIbcEurekaTestSuite go run main.go

# Exclude specific test suites
TEST_EXCLUSIONS=TestWithCosmosRelayerTestSuite,TestWithMultichainTestSuite go run main.go
```

## Environment Variables

- `TEST_NAME`: Filter to a specific test method name
- `TEST_ENTRYPOINT`: Filter to a specific test suite
- `TEST_EXCLUSIONS`: Comma-separated list of test suites to exclude

## Output

The tool outputs a JSON object compatible with GitHub Actions matrix strategy:

```json
{
  "include": [
    {"test": "Test_Deploy", "entrypoint": "TestWithIbcEurekaTestSuite"},
    {"test": "Test_2_ConcurrentRecvPacketToEth", "entrypoint": "TestWithRelayerTestSuite"}
  ]
}
```

## Test Discovery

The tool discovers tests by:

1. Parsing Go files using the Go AST parser
2. Finding top-level test functions that match `TestWith*TestSuite`
3. Finding suite method tests that match `func (s *SuiteName) Test*`
4. Combining them into the format `TopLevelTest/TestMethod`

## Running Tests

```bash
go test
```