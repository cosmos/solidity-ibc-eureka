# Native CometBFT Light Client Scope

## Supported production target

The native CometBFT light client is intentionally scoped to adjacent updates for a CometBFT chain whose validator set uses the `secp256k1eth` key type.

Supported update messages must satisfy all of the following:

- The trusted height is an IBC height `(revisionNumber, revisionHeight)` whose revision number matches the client state.
- The new CometBFT header is adjacent to the trusted height: `header.height == trustedHeight.revisionHeight + 1`.
- The client state's revision number is maintained at the IBC layer because CometBFT headers carry only a chain height.
- The validator set hash uses normal CometBFT `ValidatorSet.Hash()` semantics over `SimpleValidator{pub_key, voting_power}`.
- Each validator entry carries the compressed secp256k1 public key, a `y` witness, and voting power. The contract checks the witness is on secp256k1, matches the compressed-key parity, derives the Ethereum address, and then verifies commit signatures with `ecrecover`.
- Commit signatures are CometBFT `secp256k1eth` signatures encoded as 65 bytes `[R || S || V]`, with `V` in `{0,1}` and low-`S` enforced by the ECDSA utility.
- Header and commit-signature nanosecond components must be less than `1e9`.
- The stored consensus-state root is the CometBFT header `appHash`; membership and non-membership proofs verify against this root using the supported native ICS-23 subset.

## Native proof ABI

The native client proof bytes are an ABI encoding of `ICometBFTMsgs.ICS23Proof`, a Solidity-friendly representation of the ICS-23 `MerkleProof`/`CommitmentProof` model.

The first supported proof subset is intentionally narrow:

- Membership proofs must contain one existence `ICS23CommitmentProof` per router path segment.
- Membership proof values must be non-empty. Empty-value membership is not needed by the router/client-state paths currently supported by this native ABI.
- Non-membership proofs must contain a non-existence proof for the leaf path segment, followed by existence proofs for any parent path segments.
- Router paths are root-to-leaf, for example `[ibc, clients/...]`; native proof entries are leaf-to-root and are bound by matching proof `i` to path `path.length - 1 - i`.
- Batch and compressed ICS-23 proofs are rejected until explicitly implemented.
- The router passes `ICS24Host.prefixedPath(cInfo.merklePrefix, path)`, so the native proof verifier must bind the decoded ICS-23 proof to those exact path segments and to the stored consensus-state root.

## Explicitly out of scope for the current prototype

- Non-adjacent/light-client skipping updates.
- Validator key types other than `secp256k1eth`.
- Consensus-breaking validator-set hash changes that commit Ethereum addresses instead of CometBFT public keys.
- Non-adjacent misbehaviour evidence that requires skipping verification remains out of scope.

## Strictness differences from CometBFT verification

- The Solidity client accepts only adjacent updates. CometBFT's light verifier can verify non-adjacent updates under the usual trust-level rules.
- The client-state `trustLevel` is validated during initialization but is not used for adjacent updates. Adjacent updates require more than two-thirds signed voting power and do not perform bisection or trust-level skipping verification.
- IBC revision numbers are checked by Solidity but are not present in CometBFT headers.
- The client rejects malformed timestamp nanoseconds before computing protobuf-compatible header and vote sign bytes.
- The initial client state must have a non-empty chain ID, a non-zero latest height, a trust level between one-third and one, positive `trustingPeriod`, `unbondingPeriod`, and `maxClockDrift`, and `trustingPeriod < unbondingPeriod`.

## Completion gates for follow-up milestones

1. **Update hardening complete**: constructor validation, full-height consensus-state keys, revision-number checks, timestamp-nanos checks, and negative Foundry tests are in place.
2. **Membership complete**: ICS-23 membership proofs verify against stored `(revisionNumber, revisionHeight)` consensus roots and return the consensus timestamp in seconds.
3. **Non-membership complete**: absence proofs needed by packet timeout flows verify against stored `(revisionNumber, revisionHeight)` consensus roots and return the consensus timestamp in seconds.
4. **Misbehaviour complete**: valid conflicting headers or time-monotonicity evidence freezes the client, and all update/proof/misbehaviour entry points reject while frozen.
5. **Fixture/e2e complete**: deterministic synthetic vectors, real IBC proof vectors, router-level tests, differential vectors, fuzz tests, and gas tracking run reproducibly in CI.
