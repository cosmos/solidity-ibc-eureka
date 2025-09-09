# IBC Attestor pickup document
Here we summarise the top features, fixes and refactors that need completing in the IBC attestor stack to get it closer to production readiness.

## Features
### IBC Attestor
- Use a configurable secrets path for the secret key

### Light clients
- Implement entire 08 client interface

### CI
- Use cargo chef + cached actions for the image build

## Refactor
### IBC Attestor
- Use a single implementation for EVM chains
- Consider a relayer-styled modular approach

### Aggregator layer
- Remove dependency on `aggregator.proto` generated types
- Simplify configuration by reducing nesting

## Fixes
### CI
- Get attestors into the test suite minimal and full test suites

### E2E
- Improve relayer port allocation as discussed [here](https://github.com/cosmos/solidity-ibc-eureka/pull/748)

