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
- Get attestors into the test suite. Started [here](https://github.com/cosmos/solidity-ibc-eureka/pull/748)

### IBC Attestor
- Use attested height in membership check. Started [here](https://github.com/cosmos/solidity-ibc-eureka/pull/751)
