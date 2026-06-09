# Native CometBFT Light Client Implementation Report

## Executive Summary

This branch implements a native Solidity CometBFT light client for the supported production target documented in `docs/adr/native-cometbft-light-client.md`: adjacent CometBFT updates, validator sets using CometBFT's `secp256k1eth` key type, native ICS-23 membership and non-membership verification, router proof flows, and adjacent-scope misbehaviour freezing.

The implementation is intentionally not a full general-purpose CometBFT light client. It does not implement non-adjacent skipping verification and it does not support arbitrary CometBFT validator key types. Within the supported target, the client performs real CometBFT-compatible header, validator-set, commit-signature, app-root, and ICS-23 proof verification in Solidity.

The key enabling dependency is the local CometBFT branch:

- Repository: `/Users/gg/code/contrib/cometbft`
- Branch: `gjermund/moar-ethsecp`
- Relevant commit: `53171bf261ee80504c9faac62fe06451aca75773`

That branch makes CometBFT capable of running validator sets whose public keys are normal CometBFT consensus public keys for hashing and protobuf encoding, while their signatures are Ethereum-recoverable secp256k1 signatures that Solidity can verify with `ecrecover`.

## Repository Context

The Solidity implementation lives in:

- Repository: `/Users/gg/code/contrib/solidity-ibc-eureka`
- Branch: `gjermund/cometbft-lc-poc`
- Main contract: `contracts/light-clients/cometbft/CometBFTClient.sol`
- Proof verifier: `contracts/light-clients/cometbft/utils/CometBFTICS23.sol`
- Message types: `contracts/light-clients/cometbft/msgs/ICometBFTMsgs.sol`
- Tests: `test/cometbft/CometBFTClient.t.sol` and `test/cometbft/CometBFTRouter.t.sol`
- Update fixture generator: `scripts/cometbft-fixture/main.go`
- Native ICS-23 fixture converter: `scripts/native-ics23-fixture/main.go`

The implementation is meant to answer whether a native Solidity CometBFT light client is feasible when CometBFT validator keys are Ethereum-compatible but still participate in normal CometBFT consensus hashing.

## Supported Scope

The current client supports:

- Adjacent updates where `header.height == trustedHeight.revisionHeight + 1`.
- Full IBC height checks using both `revisionNumber` and `revisionHeight`.
- `secp256k1eth` CometBFT validators with compressed SEC1 public keys.
- On-chain validator address derivation from the validator public key and `y` witness.
- CometBFT validator-set hash recomputation over public keys and voting power.
- CometBFT header hash recomputation over the supported header fields.
- Basic commit signature validation over the full validator set.
- Ethereum ECDSA recovery until signed voting power exceeds two-thirds.
- Consensus-state storage keyed by full IBC height.
- ICS-23 membership verification against the stored CometBFT app hash.
- ICS-23 non-membership verification for timeout-style absence proofs.
- `ICS26Router` packet commitment, acknowledgement, and timeout proof flows.
- Misbehaviour freezing for adjacent-scope signed conflicting headers and time monotonicity violations.

The current client does not support:

- Non-adjacent CometBFT skipping verification or bisection.
- Validator key types other than `secp256k1eth`.
- Raw protobuf ICS-23 decoding on-chain.
- General-purpose ICS-23 proof variants outside the implemented Cosmos SDK IAVL plus Tendermint multistore subset.

## How Update Verification Works

`CometBFTClient.updateClient` accepts an ABI-encoded native update message. The message includes the trusted IBC height, a CometBFT header, the next validator set, and commit signatures.

The update path performs these checks before writing state:

1. The client is not frozen.
2. The trusted consensus state exists at the full `(revisionNumber, revisionHeight)` key.
3. The trusted revision number matches the client state's revision number.
4. The new header is adjacent to the trusted height.
5. Header and commit-signature timestamp nanoseconds are valid.
6. The existing trusted state is not expired.
7. The new header time is not too far in the future under `maxClockDrift`.
8. The supplied validator set hashes to the header's `nextValidatorsHash`.
9. The CometBFT header hash recomputed in Solidity matches the commit block ID.
10. Commit signatures recover to validators in the supplied set.
11. Signed voting power is greater than two-thirds of total voting power.

When all checks pass, the client stores the new consensus state at the full IBC height. The stored state includes the timestamp, app root, next validator hash, and consensus-state hash used by the broader IBC client interface.

This is why the realistic 20-validator update gas is in the low hundreds of thousands, not the much larger Foundry test-function gas number. The actual contract call does meaningful work: ABI-decodes a large nested update, recomputes CometBFT hashes, validates commit signatures, and recovers signatures until quorum.

## How Validator Verification Works

The CometBFT branch provides validators with `secp256k1eth` public keys. Solidity receives the compressed public key plus a `y` coordinate witness. The contract:

1. Checks the point is on secp256k1.
2. Checks the `y` parity matches the compressed public-key prefix.
3. Derives the Ethereum address as `keccak256(x || y)[12:]`.
4. Uses `ecrecover` over the CometBFT vote sign bytes to recover commit signers.
5. Matches recovered addresses against validators derived from the public keys.

The important design point is that validator-set hashing remains CometBFT-compatible. The validator-set hash is computed over CometBFT `SimpleValidator{pub_key, voting_power}` data, not over Ethereum addresses. Ethereum addresses are used only for signature recovery and signer matching.

## How Native ICS-23 Verification Works

`CometBFTICS23.sol` implements the native proof verification path.

The on-chain proof is not raw protobuf. Instead, fixture tooling converts real ICS-23 proofs into a Solidity-friendly ABI representation:

- `ICometBFTMsgs.ICS23Proof`
- `ICS23CommitmentProof`
- `ICS23ExistenceProof`
- `ICS23NonExistenceProof`
- `ICS23LeafOp`
- `ICS23InnerOp`

Membership verification checks:

- The proof decodes into the expected native ABI type.
- Proof entries are bound to router path segments.
- The leaf key and value match the requested path and value.
- IAVL proof operations use the supported leaf, inner-op, length, and hash settings.
- The Tendermint store-root proof links the IAVL root to the CometBFT app hash.
- The final calculated root equals the consensus state's stored `appHash`.

Non-membership verification checks:

- The first proof entry is an IAVL non-existence proof for the requested leaf path.
- Left and/or right neighbor proofs are valid when present.
- Neighbor ordering and boundary conditions are enforced.
- Parent proof entries link the absence proof to the stored CometBFT app hash.

This is sufficient for the currently supported router flows, including packet receipt absence for timeouts.

## Router Integration

Router-level coverage lives in `test/cometbft/CometBFTRouter.t.sol`.

Those tests exercise the native client through `ICS26Router` for:

- Packet commitment membership.
- Acknowledgement commitment membership.
- Packet receipt non-membership for timeout.

The tests use native fixtures derived from real Cosmos SDK e2e proof data. The local router state is seeded as needed to put `ICS26Router` into the correct test state, but the remote proof verification itself goes through the native CometBFT client and the generated proof fixtures.

## Fixture And E2E Story

There are two fixture paths.

The update and misbehaviour fixture generator is:

- `scripts/cometbft-fixture/main.go`

It uses the local CometBFT dependency to:

- Create `secp256k1eth` validator sets.
- Sign CometBFT vote sign bytes.
- Run CometBFT adjacent verification in Go.
- Emit Solidity fixtures for valid updates, 20-validator updates, and misbehaviour cases.

The native ICS-23 fixture converter is:

- `scripts/native-ics23-fixture/main.go`

It consumes e2e-derived proof source data, reference-verifies the proofs in Go, converts them into the native Solidity ABI shape, and writes Foundry fixtures.

The e2e source fixture flow is:

- `e2e/interchaintestv8/cosmos_proof_api_test.go`
- `e2e/interchaintestv8/types/tendermint_light_client_fixtures.go`
- `e2e/interchaintestv8/types/tendermint_light_client_fixtures/*.go`

That path broadcasts real relay transactions, extracts IBC v2 app acknowledgements, queries real proof data, and writes packet commitment, acknowledgement commitment, and packet receipt absence source fixtures.

The `justfile` ties the paths together:

- `generate-fixtures-tendermint-light-client` runs the heavier e2e fixture generation path.
- `generate-cometbft-fixtures` regenerates native Solidity fixtures.
- `check-cometbft-fixtures` checks committed fixture drift and verifies the local CometBFT dependency is pinned to the expected commit.
- `test-foundry-cometbft` runs the focused native CometBFT Foundry suite.

## Why The Current CometBFT Branch Makes This Possible

The feasibility hinges on the `secp256k1eth` key type added in `/Users/gg/code/contrib/cometbft`.

The key implementation is:

- `/Users/gg/code/contrib/cometbft/crypto/secp256k1eth/key.go`

It provides:

- 33-byte compressed SEC1 public keys.
- Ethereum address derivation from the uncompressed public key.
- Signatures encoded as `[R || S || V]`.
- `V` values in `{0,1}`.
- Low-`S` enforcement.
- Legacy Keccak-256 signing compatible with Ethereum recovery.

CometBFT protobuf encoding support is added through:

- `/Users/gg/code/contrib/cometbft/crypto/encoding/codec.go`

The decisive property is that CometBFT still hashes validators as CometBFT public keys and voting power. In CometBFT, `ValidatorSet.Hash()` commits to `SimpleValidator{pub_key, voting_power}`. The branch keeps that behavior intact for `secp256k1eth`.

That gives Solidity exactly the bridge it needs:

- CometBFT consensus commits to public keys and voting power in the normal validator-set hash.
- The same public keys can be represented compactly and checked on-chain.
- Signatures are Ethereum-recoverable, so Solidity can use `ecrecover`.
- Recovered Ethereum addresses can be compared to addresses derived from the validator public keys.
- The contract does not need any consensus-breaking shortcut such as hashing Ethereum addresses instead of CometBFT public keys.

Without this CometBFT branch, the native Solidity path would be much less practical. Standard CometBFT validators commonly use key/signature schemes that Solidity cannot verify cheaply with precompiles. The branch preserves CometBFT's consensus object model while making the commit signatures verifiable on Ethereum.

## Verification State

The implementation has been validated with the focused native CometBFT suite and fixture checks:

```bash
just check-cometbft-fixtures
just test-foundry-cometbft
forge test --match-path 'test/cometbft/*' -vvv
just build-contracts
```

Additional validation used during development included:

```bash
forge fmt --check
cargo fmt --check
git diff --check
cargo check -p proof-api --bin proof-api --locked
cargo check -p proof-api --bin proof-api --no-default-features --features cosmos-to-cosmos --locked
```

Focused gas observations from the native CometBFT path:

- `updateClient` for the 20-validator fixture is about `484k` gas for the actual call path.
- Broader gas reports have observed hot-path maxima around `644k` for `updateClient`, `77k` for membership, `110k` for non-membership, and `421k` for misbehaviour, depending on the specific fixture/test path.

Large Foundry per-test gas numbers should not be read as contract-call gas. They can include JSON fixture parsing, construction of large nested structs in Solidity tests, client deployment, and assertions.

## Current Caveats And Remaining Boundaries

The implemented client is complete for the scoped native target, but these boundaries remain important:

- Non-adjacent skipping verification is still out of scope.
- Non-`secp256k1eth` validator sets are still out of scope.
- The native proof ABI is a Solidity-friendly converted ICS-23 representation, not raw protobuf proof bytes.
- The full e2e fixture generation path is intentionally heavier than normal local checks; CI-oriented validation relies on committed source fixtures and `check-cometbft-fixtures`.
- Solana proof-api modules are separate from the native CometBFT target. Solana feature builds still depend on generated Solana SDK artifacts being present.

## Bottom Line

The implementation demonstrates that a native Solidity CometBFT light client is feasible for an adjacent-update, `secp256k1eth` CometBFT chain. The current CometBFT branch makes this possible by preserving normal CometBFT validator-set hashing while producing Ethereum-recoverable consensus signatures. Solidity can therefore verify the same committed validator set and recover commit signers without changing CometBFT's consensus hash semantics.
