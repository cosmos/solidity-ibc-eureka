# End to End Testing Suite with Interchaintest

The e2e tests are built using the [interchaintest](https://github.com/strangelove-ventures/interchaintest) library by Strangelove. It runs multiple docker container validators, and lets you test IBC enabled smart contracts.

These end to end tests are designed to run in the CI, but you can also run them locally.

## Running the tests locally

### Prerequisites

In the repo root:

```
cp .env.example .env
just install-relayer && just install-operator
```

### Run tests

To run the tests locally, run the following commands from the repo root:

```shell
# Run the following where `$TEST_NAME` is one of the test names of the `$TEST_SUITE_FN`:
just test-e2e $TEST_SUITE_FN/$TEST_NAME

# For example, to run the `TestDeploy` test, you would run:
just test-e2e TestWithIbcEurekaTestSuite/Test_Deploy

# Altneratively:
just test-e2e-eureka Test_Deploy
```
