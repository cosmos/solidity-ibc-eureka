# Native CometBFT Light Client Implementation Report

## Summary

The native CometBFT light client in this branch of `solidity-ibc-eureka` is implemented as a scoped Solidity light client for adjacent CometBFT updates using `secp256k1eth` validator keys.

The supported target is intentionally narrower than the full general CometBFT light-client algorithm. It supports adjacent updates, native ICS-23 membership and non-membership proofs, router packet/acknowledgement/timeout proof flows, and adjacent-scope misbehaviour freezing. Non-adjacent skipping verification and validator key types other than `secp256k1eth` remain out of scope.

## Core Contract

The main implementation is:

- `contracts/light-clients/cometbft/CometBFTClient.sol`

`updateClient` ABI-decodes a native update message, validates the trusted consensus state, validates the header and validator set, verifies the commit signatures, and stores the new consensus state keyed by full IBC height.

Important mechanics:

- `updateClient` performs trusted-state, header/validator, and commit verification before storing the new consensus state.
- `verifyMembership` and `verifyNonMembership` fetch the consensus state for the requested full IBC height and verify the proof against the stored app hash.
- `misbehaviour` validates two signed update headers and freezes the client if they conflict or violate time monotonicity.
- Adjacent-only verification is enforced by requiring `header.height == trustedHeight.revisionHeight + 1`.
- The contract recomputes the CometBFT validator-set hash and header hash rather than trusting fixture fields.
- Commit signatures are checked until signed voting power exceeds two-thirds.

Validator addresses are derived on-chain from compressed secp256k1 public keys. Each validator carries the compressed public key plus a `y` witness. The contract checks the point is on secp256k1, checks parity against the compressed-key prefix, and derives the Ethereum address as `keccak256(x || y)[12:]`.

## Native ICS-23 Proofs

The ICS-23 verifier is:

- `contracts/light-clients/cometbft/utils/CometBFTICS23.sol`

It verifies a deliberately narrow native ABI representation of ICS-23:

- Membership: IAVL leaf existence proof plus Tendermint store-root proof.
- Non-membership: IAVL non-existence proof plus Tendermint store-root proof.
- Proof entries are bound to the router/client path segments.
- IAVL leaf, inner-op, prefix/suffix, hash-op, and neighbor-order constraints are checked.
- The calculated app root must match the consensus state's `appHash`.

This is not raw protobuf ICS-23 on-chain. The fixture tooling converts real ICS-23 proofs into a Solidity-friendly ABI shape.

## Router Integration

Router-level tests are in:

- `test/cometbft/CometBFTRouter.t.sol`

They verify that the native client can be used by `ICS26Router` for:

- packet commitment membership,
- acknowledgement commitment membership,
- packet receipt absence for timeout.

These tests use e2e-derived proof data, not mock proofs. Some local router state is seeded in tests to reach the relevant router path, but the remote proof verification itself goes through the native CometBFT light client and real generated proof fixtures.

## Fixtures And E2E Path

The update/misbehaviour fixture generator is:

- `scripts/cometbft-fixture/main.go`

It uses the local CometBFT branch to create `secp256k1eth` validator sets, sign CometBFT vote sign bytes, run `light.VerifyAdjacent`, and emit vectors for Solidity.

The native ICS-23 fixture converter is:

- `scripts/native-ics23-fixture/main.go`

It consumes real e2e source fixtures, reference-verifies the ICS-23 proofs in Go, ABI-encodes them into the Solidity proof shape, and writes native Foundry fixtures.

The e2e fixture source is generated in:

- `e2e/interchaintestv8/cosmos_proof_api_test.go`

The packet/acknowledgement path broadcasts real relay transactions, extracts the IBC v2 app acknowledgement, queries real proof data, and writes packet commitment, acknowledgement commitment, and packet receipt absence fixtures.

The `justfile` wires this up:

- `generate-fixtures-tendermint-light-client` runs the heavy e2e fixture generation.
- `generate-cometbft-fixtures` regenerates native fixtures from committed source data.
- `check-cometbft-fixtures` checks committed fixture drift and verifies the local CometBFT dependency is pinned to the expected commit.

## Why The Current CometBFT Branch Makes This Possible

The local CometBFT dependency is:

- Repo: `/Users/gg/code/contrib/cometbft`
- Branch: `gjermund/moar-ethsecp`
- Commit: `53171bf261ee80504c9faac62fe06451aca75773`
- Upstream branch: `origin/gjermund/moar-ethsecp`

That branch adds an Ethereum-compatible CometBFT validator key type: `secp256k1eth`.

The key implementation is:

- `/Users/gg/code/contrib/cometbft/crypto/secp256k1eth/key.go`

It provides:

- 33-byte compressed SEC1 public keys.
- Ethereum address derivation: `Keccak256(uncompressedPubKey[1:])[12:]`.
- signatures as `[R || S || V]`, with `V` in `{0,1}`.
- legacy Keccak-256 signing, matching Ethereum/go-ethereum behavior.
- low-`S` signature enforcement.

CometBFT encoding support is added through:

- `/Users/gg/code/contrib/cometbft/crypto/encoding/codec.go`

That code can convert `secp256k1eth` public keys to and from CometBFT protobuf public keys.

The critical feasibility point is that the branch keeps validator-set hashing compatible with normal CometBFT public-key hashing. CometBFT hashes `SimpleValidator{pub_key, voting_power}` in `/Users/gg/code/contrib/cometbft/types/validator.go`. That means Solidity can reproduce CometBFT's `ValidatorSet.Hash()` by encoding the same public keys and voting powers, without relying on Ethereum addresses as the validator-set hash input.

That property is what makes the Solidity implementation viable:

- CometBFT consensus sees validator public keys and voting power.
- Commit signatures are Ethereum-recoverable because the key type signs with Ethereum-compatible secp256k1.
- Solidity can recompute the validator-set hash using CometBFT public-key bytes.
- Solidity can recover Ethereum addresses from signatures and compare them to the addresses derived from the same public keys.
- No consensus-breaking "hash validator addresses instead of pubkeys" shortcut is needed.

## Verification State

The implementation was verified with:

- `just test-foundry-cometbft`
- `forge test --match-path 'test/cometbft/*' -vvv`
- `just check-cometbft-fixtures`
- `just build-contracts`
- proof-api default and Cosmos-only cargo checks/builds
- Go tests for native fixture conversion and e2e fixture types
- `forge fmt --check`
- `cargo fmt --check`
- `git diff --check`
- focused gas report

Observed hot-path gas:

- `updateClient`: max `644,252`
- `verifyMembership`: `76,896`
- `verifyNonMembership`: `109,846`
- `misbehaviour`: `420,617`

## Current Caveats

- This is complete for the documented native CometBFT target, not for generalized non-adjacent CometBFT skipping verification.
- Validator key types other than `secp256k1eth` are not supported.
- Full e2e fixture regeneration is intentionally a heavier manual path. CI checks committed fixture drift through `just check-cometbft-fixtures`.
- The native proof ABI is a Solidity-friendly converted proof format, not raw protobuf ICS-23 bytes.
