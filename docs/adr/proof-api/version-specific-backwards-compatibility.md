# ADR: Version-Specific Proof API Backwards Compatibility

**Status**: Implemented
**Date**: 2026-06-17

## Context

Proof API modules construct IBC transactions for a specific source chain, destination chain, light client, and proof format. These formats can change between light client releases. When a deployed chain continues to use an older light client, the proof API must still be able to build transactions that match that older client's expected message and proof format.

The Ethereum to Cosmos path needed support for the `cw-ics08-wasm-eth-v1.3.0` wasm light client after the main `eth_to_cosmos` proof API module had moved on. The first solution was a generic compatibility module that could route to either the old v1.3.0 implementation or the current implementation based on the client checksum. That worked, but it made one module responsible for two behaviors and hid which proof format a deployment was actually using.

## Decision

Backward compatibility for old proof formats must be implemented with version-specific proof API modules, not generic compatibility modules.

For Ethereum to Cosmos v1.3.0, the module is named:

```text
eth_to_cosmos_v1.3.0
```

The Rust crate and path use Rust- and Cargo-safe names:

```text
proof-api-eth-to-cosmos-v1-3-0
packages/proof-api/modules/eth-to-cosmos-v1-3-0
EthToCosmosV1_3_0ProofApiModule
```

The version-specific module supports only that legacy version. It must reject requests for other versions instead of falling back to the current module.

The current module remains registered separately:

```text
eth_to_cosmos
```

This makes the selected proof format explicit in configuration and prevents legacy behavior from leaking into the main module.

## Implementation Pattern

### 1. Identify The Compatibility Boundary

Choose a deterministic compatibility key for the legacy behavior. For wasm light clients, use the wasm checksum because it identifies the exact code deployed on chain.

For `cw-ics08-wasm-eth-v1.3.0`, the checksum is:

```text
af84cccca3e746d9c4ea980c6d1b4511de0fa962ed5003dee8cb44eda10e4568
```

Use this checksum for runtime validation. Do not rely only on release tags or environment variables, because those are deployment inputs and not on-chain truth.

### 2. Pin The Legacy Implementation

If the old proof construction code still exists in a historical revision, depend on that exact revision under an aliased workspace dependency.

For Ethereum to Cosmos v1.3.0, the proof API module delegates to the legacy relayer module pinned to the v1.3.0 revision:

```toml
ibc-eureka-relayer-eth-to-cosmos-v1_3 = { package = "ibc-eureka-relayer-eth-to-cosmos", git = "https://github.com/cosmos/solidity-ibc-eureka", rev = "035aa5eb171b6608614a9bc4e76e79d61c190e39" }
ibc-eureka-relayer-core-v1_3 = { package = "ibc-eureka-relayer-core", git = "https://github.com/cosmos/solidity-ibc-eureka", rev = "035aa5eb171b6608614a9bc4e76e79d61c190e39" }
```

The main module must not absorb legacy proof construction code unless there is no isolated legacy implementation available.

### 3. Translate At The Module Boundary

The version-specific module should still implement the current `ProofApiService` trait so the proof API binary exposes one current gRPC API.

Inside the module, translate current proof API request and response types into the legacy implementation's request and response types. Keep this translation local to the version-specific module.

This keeps legacy protobuf or transaction-building differences out of the main module and out of callers.

### 4. Validate The Version Before Building Transactions

For create-client requests, validate the requested checksum parameter before delegating:

```text
parameters["checksum_hex"] == af84cccca3e746d9c4ea980c6d1b4511de0fa962ed5003dee8cb44eda10e4568
```

For requests that operate on an existing destination client, query the destination chain's client state, decode the wasm client state, and compare the on-chain checksum.

Requests with a non-matching checksum must fail with a clear error. They must not fall back to the current module.

### 5. Register Both Modules

The proof API binary should register both the current module and the version-specific module:

```rust
proof_api_builder.add_module(EthToCosmosProofApiModule);
proof_api_builder.add_module(EthToCosmosV1_3_0ProofApiModule);
```

Configuration chooses which module handles a chain pair.

## Naming Rules

Runtime module names should include the user-facing semantic version:

```text
<direction>_v<major>.<minor>.<patch>
eth_to_cosmos_v1.3.0
```

Rust and Cargo names should use safe separators:

```text
proof-api-eth-to-cosmos-v1-3-0
EthToCosmosV1_3_0ProofApiModule
```

Do not use `compat` in names when the module is tied to a specific legacy version. The name should say which version it supports.

## Testing Requirements

Version-specific compatibility modules should include tests for:

1. The checksum byte constant matching the expected hex string.
2. Config generation selecting the version-specific module for the exact legacy artifact tag.
3. Config generation selecting the current module for `local`, empty, and unrelated artifact tags.
4. Rejection of unsupported modes such as attested mode when the legacy implementation does not support them.

When possible, add integration coverage that creates or updates a client using the legacy artifact.

## Anti-Patterns

- Do not implement a generic `*_compat` module that routes to old or current implementations at runtime.

- Do not make the current module understand legacy proof formats unless the legacy format is still the canonical current behavior.

- Do not select the legacy implementation solely from an environment variable or release tag at runtime. Use environment variables only for test/config generation. Validate against the actual client checksum when serving requests.

- Do not support both current and legacy behavior from one version-specific module. A module named `eth_to_cosmos_v1.3.0` should only support v1.3.0.

- Do not keep compatibility modules indefinitely without a known consumer. Remove version-specific modules after no supported deployment uses that legacy light client.

## Consequences

### Positive

- The selected proof format is visible in proof API configuration.

- The main module remains focused on current behavior.

- Legacy code is isolated and easier to delete when no longer needed.

- Checksum validation prevents accidental use of the wrong proof format with a deployed client.

- e2e tests can cover old and current light clients by changing configuration rather than changing runtime routing logic.

### Negative

- Each incompatible legacy version needs its own module and registration.

- The proof API binary may carry multiple implementations for the same chain direction while legacy deployments are supported.

- Request and response conversion code is duplicated per legacy version when legacy protobuf types differ from current proof API types.

## Current Example

The implemented example is Ethereum to Cosmos support for `cw-ics08-wasm-eth-v1.3.0`:

```text
packages/proof-api/modules/eth-to-cosmos-v1-3-0
programs/proof-api/src/bin/proof-api.rs
e2e/interchaintestv8/proofapi/builder.go
```

This module exposes the current proof API gRPC interface, delegates transaction construction to the pinned v1.3.0 relayer implementation, validates the v1.3.0 checksum, and rejects non-v1.3.0 clients instead of falling back to `eth_to_cosmos`.
