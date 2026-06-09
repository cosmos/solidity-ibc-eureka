# Native CometBFT Light Client Implementation Plan

## Context

This plan is for the native Solidity CometBFT light client prototype in:

- Solidity repo: `/Users/gg/code/contrib/solidity-ibc-eureka`
- Branch: `gjermund/cometbft-lc-poc`
- Main contract: `contracts/light-clients/cometbft/CometBFTClient.sol`
- Update fixture generator: `scripts/cometbft-fixture/main.go`
- Native ICS-23 fixture generator: `scripts/native-ics23-fixture/main.go`

The prototype depends on a local CometBFT branch that adds the `secp256k1eth` validator key type and keeps validator-set hashing compatible with normal CometBFT public-key hashing:

- CometBFT repo: `/Users/gg/code/contrib/cometbft`
- Branch: `gjermund/moar-ethsecp`
- Current relevant head during planning: `53171bf261 fix!: keep secp256k1eth validator hashes on pubkeys`
- Upstream verification: `https://github.com/cometbft/cometbft` has `refs/heads/gjermund/moar-ethsecp` at `53171bf261ee80504c9faac62fe06451aca75773`.

The current Solidity client already verifies adjacent CometBFT updates for chains whose validator set uses `secp256k1eth` validators. It ABI-decodes an update message, checks the trusted consensus-state, checks revision number and adjacent height, verifies CometBFT validator-set and header hashes, validates basic commit signatures, recovers Ethereum secp256k1 signers, and stores the new full consensus state by full IBC height.

This plan makes the client fully usable as an IBC light client for the current supported production target: adjacent updates, `secp256k1eth` validator sets, ICS-23 membership proofs against the CometBFT app hash, non-membership proofs for timeout flows, explicit misbehaviour freezing, deterministic fixtures, and router-level end-to-end tests. Non-adjacent skipping updates and non-`secp256k1eth` validator key types remain separate scope extensions unless the production target changes.

## Current Status On This Branch

Implemented now:

- Adjacent update verification for generated 3-validator and 20-validator CometBFT fixtures.
- Full consensus-state storage keyed by full IBC height.
- Native Solidity ICS-23 proof ABI and parser checks.
- Direct `verifyMembership` against stored consensus roots using generated synthetic and real CometBFT/IBC proof fixtures.
- Direct `verifyNonMembership` against stored consensus roots using a real CometBFT/IBC proof fixture.
- Router-level `ICS26Router` tests for packet commitment membership, acknowledgement membership, and packet receipt non-membership using native fixtures derived from Cosmos SDK e2e packet flows.
- Explicit misbehaviour evidence for same-height conflicting headers and time-monotonicity violations, with signed CometBFT evidence fixtures and frozen-client entrypoint coverage.
- Portable native ICS-23 fixture regeneration through `scripts/native-ics23-fixture`, with `just check-cometbft-fixtures` wired into Foundry CI.
- Differential vector assertions for validator-set hashes, header hashes, vote sign bytes, recovered signers, proof roots, paths, values, and acknowledgement commitments.
- Focused fuzz coverage for trusted revision mutation, timestamp nanosecond bounds, and proof-height lookup failures.

Still remaining outside the supported light-client target:

- Non-adjacent/skipping updates.
- Validator key types other than `secp256k1eth`.
- Solana proof-api modules remain opt-in for this work. The default proof-api build and native CometBFT fixture path exclude Solana generated SDK modules; enabling Solana features still requires those generated modules to be present.

Execution checkpoint:

- Phases 0 through 7 are implemented in the current working tree.
- Phase 8 is implemented for the supported target: focused CometBFT tests run under `foundry-cometbft`, fixture drift checks run locally and in CI, differential fixture assertions are in the Foundry suite, and targeted fuzz tests run under `just test-foundry-cometbft`.
- Phase 9 is the merge-readiness review gate: run the final validation commands, review the gas report, and resolve any independent reviewer findings.

## Completion Definition

The light client is complete when all of these are true:

- `updateClient` accepts valid adjacent CometBFT headers and rejects malformed, expired, future, wrong-revision, wrong-chain, wrong-validator-set, wrong-commit, insufficient-quorum, and replay/conflict cases.
- Consensus states are stored by full IBC height and expose enough state to verify proofs: timestamp, app root, next validator hash, and hash.
- `verifyMembership` verifies real ICS-23 membership proofs against the stored consensus root for the requested full IBC height and returns the consensus timestamp in seconds.
- `verifyNonMembership` verifies real ICS-23 absence proofs against the stored consensus root for the requested full IBC height and returns the consensus timestamp in seconds.
- `misbehaviour` freezes the client for valid evidence and every update/proof entry point rejects while frozen.
- Router-level tests prove that native CometBFT proof verification works with the exact path/value shapes used by `ICS26Router`.
- Fixture generation is reproducible from Go e2e-style flows, not just hand-built Solidity structs.
- Tests include deterministic vectors, negative vectors, router/e2e fixtures, fuzz/property checks where useful, gas reports for the hot paths, and CI-friendly commands.
- Each implementation phase ends with independent sub-agent review for correctness, unnecessary complexity, and test quality, followed by passing build/lint/test commands.

## Phase 0: Baseline And Scope Lock

Goal: make the current branch a clean baseline before adding proof logic.

Implementation tasks:

- Confirm the current uncommitted update-hardening changes are intentional and either commit them or keep them as the explicit base diff for later phases.
- Preserve the ADR at `docs/adr/native-cometbft-light-client.md` as the scope contract: adjacent updates, `secp256k1eth`, full-height consensus keys, native membership/non-membership, and adjacent-scope misbehaviour freezing.
- Run the current CometBFT fixture generator and focused Foundry suite to ensure no hidden drift from `/Users/gg/code/contrib/cometbft`.
- Record the current gas for the 3-validator and 20-validator update tests.

Validation commands:

```bash
bun install --frozen-lockfile
forge fmt --check contracts/light-clients/cometbft/CometBFTClient.sol contracts/light-clients/cometbft/errors/ICometBFTClientErrors.sol test/cometbft/CometBFTClient.t.sol
forge test --config-path foundry-cometbft/foundry.toml -vvv
forge test --config-path foundry-cometbft/foundry.toml --match-test test_twentyValidatorValidAdjacentUpdateClient --gas-report
just test-foundry-cometbft
just build-contracts
git diff --check
go -C /Users/gg/code/contrib/cometbft test ./crypto/secp256k1eth ./types
```

Phase gate:

- Spawn three independent reviewers: update-correctness/security, simplicity/storage design, and test-quality/fixtures.
- Address all high-confidence findings before starting Phase 1.

## Phase 1: Store Full Consensus States

Goal: proof verification must not rely only on consensus-state hashes.

Implementation tasks:

- Add storage for the full `ICometBFTMsgs.ConsensusState` keyed by full IBC height.
- Keep `getConsensusStateHash(Height)` and `getConsensusStateHash(uint64)` behavior stable.
- Add a full-height consensus-state getter for tests and proof verification.
- Store the initial consensus state and every successful update's full consensus state.
- Make frozen and unknown-height checks reusable across update, membership, non-membership, and misbehaviour paths.
- Add custom errors for missing consensus state, frozen client, invalid proof height revision, and malformed proof inputs as needed.

Tests:

- Constructor stores initial hash and full state at `(revisionNumber, revisionHeight)`.
- Adjacent update stores full state and hash at the new full height.
- Same revision height under a different revision number is not accepted.
- Existing convenience hash getter still uses current revision.
- Unknown proof height reverts before proof decoding.

Validation:

```bash
forge fmt --check contracts/light-clients/cometbft/CometBFTClient.sol contracts/light-clients/cometbft/errors/ICometBFTClientErrors.sol test/cometbft/CometBFTClient.t.sol
forge test --config-path foundry-cometbft/foundry.toml -vvv
just build-contracts
git diff --check
```

Phase gate:

- Reviewers check storage compatibility, height semantics, and tests that fail on revision-key mistakes.

## Phase 2: Define The Native Proof ABI And ICS-23 Scope

Goal: freeze the exact ABI and path semantics before writing a large verifier.

Implementation tasks:

- Define native CometBFT proof message structs in `contracts/light-clients/cometbft/msgs/ICometBFTMsgs.sol`.
- Choose one encoded proof shape for both direct light-client calls and router calls. The preferred shape is `abi.encode(NativeMembershipProof)` where the inner fields are a Solidity-friendly representation of the ICS-23 `CommitmentProof`.
- Document path handling: `ICS26Router` passes `ICS24Host.prefixedPath(cInfo.merklePrefix, path)`, so the native client must verify exactly those bytes segments against the IAVL/store proof key.
- Decide and document which Cosmos proof specs are accepted first. The likely first target is the normal Cosmos SDK IBC store proof path: multistore commitment proof plus IAVL membership/non-membership proof, matching what ibc-go relayers provide.
- Add a small Go fixture tool that emits one membership proof and one non-membership proof from the same encoding the Solidity verifier expects.
- Add parser-only Solidity tests before cryptographic verification is implemented.

Tests:

- ABI decode succeeds for generated membership/non-membership fixture proofs.
- Empty or structurally invalid native ABI proof envelopes revert with a specific error.
- Unexpected proof type, inactive oneof arm population, path/proof count mismatch, and path/value binding mismatches revert.
- Cosmos IAVL/Tendermint proof-spec, hash-operation, and prefix checks are documented here but implemented with the cryptographic verifier in the membership/non-membership phases.
- Empty path, empty value for membership, and unsupported path layouts revert.

Validation:

```bash
forge test --config-path foundry-cometbft/foundry.toml --match-contract CometBFTClientTest -vvv
(cd scripts/native-ics23-fixture && go test ./...)
git diff --check
```

Phase gate:

- Reviewers check whether the ABI is minimal, whether it matches router call shapes, and whether unsupported proof variants are rejected clearly instead of silently accepted.

## Phase 3: Implement ICS-23 Membership Verification

Goal: `verifyMembership` proves a value exists under a stored CometBFT app root.

Implementation tasks:

- Add an ICS-23 verification library under `contracts/light-clients/cometbft/utils/`.
- Implement the required protobuf-style proof operations with structured decoding, not ad hoc byte slicing where a typed decoder is practical.
- Verify the commitment proof against the stored `ConsensusState.root`.
- Convert `bytes[] path` to the exact key expected by the proof according to the Phase 2 path contract.
- Compare the proved value with `msg_.value`.
- Return the stored consensus timestamp in seconds.
- Enforce `notFrozen` and `onlyProofSubmitter` consistently with update behavior unless the broader repo expects proof verification to be open.

Tests:

- Direct valid membership proof succeeds and returns the consensus timestamp in seconds.
- Wrong value, wrong path, wrong proof height, wrong revision number, wrong app root, malformed proof, and unsupported proof op all revert.
- Proof against an old but still stored height succeeds when the height is known.
- Frozen client rejects membership verification.
- Tests use generated proof fixtures, not mocks.

Validation:

```bash
forge fmt --check contracts/light-clients/cometbft/**/*.sol test/cometbft/CometBFTClient.t.sol
forge test --config-path foundry-cometbft/foundry.toml --match-test test_verifyMembership -vvv
forge test --config-path foundry-cometbft/foundry.toml -vvv
just build-contracts
git diff --check
```

Phase gate:

- Reviewers check proof soundness, root/key/value binding, gas-sensitive implementation choices, and whether negative tests would catch a verifier that accepts the wrong key or value.

## Phase 4: Implement ICS-23 Non-Membership Verification

Goal: `verifyNonMembership` proves absence for packet timeout flows.

Implementation tasks:

- Extend the ICS-23 library for non-existence proofs using the proof format selected in Phase 2.
- Verify absence against the stored app root and requested path.
- Return the stored consensus timestamp in seconds so `ICS26Router.timeoutPacket` can compare it to the packet timeout timestamp.
- Reuse height, frozen, role, path, and proof-spec checks from membership.

Tests:

- Direct valid non-membership proof succeeds and returns consensus timestamp in seconds.
- Existing key submitted as non-membership reverts.
- Neighbor proof/key-boundary tampering reverts.
- Wrong path, proof height, revision, app root, proof type, and malformed proof revert.
- Frozen client rejects non-membership verification.

Validation:

```bash
forge test --config-path foundry-cometbft/foundry.toml --match-test test_verifyNonMembership -vvv
forge test --config-path foundry-cometbft/foundry.toml -vvv
just build-contracts
git diff --check
```

Phase gate:

- Reviewers check absence-proof edge cases, especially nearest-left/right key handling and timeout timestamp behavior.

## Phase 5: Router-Level Integration Tests

Goal: prove the client works through the actual IBC router APIs, not only direct contract calls.

Implementation tasks:

- Add native CometBFT router tests that register the client with `ICS26Router`.
- Use generated CometBFT consensus/update fixtures and ICS-23 proof fixtures to exercise:
  - packet receive membership verification,
  - acknowledgement membership verification,
  - packet timeout non-membership verification.
- Ensure path construction matches `ICS24Host.prefixedPath` and the configured merkle prefix.
- Avoid mocks for light-client proof verification in these tests.

Tests:

- `recvPacket` succeeds with a valid packet commitment proof.
- `ackPacket` succeeds with a valid acknowledgement proof.
- `timeoutPacket` succeeds only when the counterparty consensus timestamp is at or after the packet timeout timestamp.
- Wrong merkle prefix, wrong path segment, wrong value, and stale timestamp fail at the router/client boundary.

Validation:

```bash
forge test --match-contract ICS26RouterCometBFTTest -vvv
forge test --config-path foundry-cometbft/foundry.toml -vvv
just test-foundry-cometbft
just build-contracts
git diff --check
```

Phase gate:

- Reviewers check that tests exercise the real router/client interface and not an easier direct-call substitute.

## Phase 6: Misbehaviour And Freezing

Goal: explicit evidence can freeze the client, and frozen clients cannot update or verify proofs.

Implementation tasks:

- Add a `MsgSubmitMisbehaviour` ABI containing two trusted heights, two trusted consensus states, two headers, two commits, and the required validator sets.
- Reuse update verification helpers to verify each header against its trusted state without storing a new consensus state.
- Freeze on valid double-sign/conflicting header evidence at the same height.
- Freeze on valid time monotonicity violation where a higher height has a timestamp less than or equal to a lower trusted height.
- Keep evidence validation strict: correct chain ID, full revision numbers, known trusted consensus states, adjacent target where required by current scope, valid commits, and quorum.
- Make `misbehaviour` `notFrozen` and `onlyProofSubmitter`, then set `clientState.isFrozen = true` for valid evidence.

Tests:

- Double-sign/conflicting headers freeze the client.
- Time-monotonicity evidence freezes the client.
- Invalid evidence does not freeze and reverts with a specific error.
- After freezing, `updateClient`, `verifyMembership`, `verifyNonMembership`, and `misbehaviour` reject.
- Misbehaviour fixtures are generated from CometBFT types where possible.

Validation:

```bash
forge test --config-path foundry-cometbft/foundry.toml --match-test test_misbehaviour -vvv
forge test --config-path foundry-cometbft/foundry.toml -vvv
just build-contracts
git diff --check
```

Phase gate:

- Reviewers check evidence soundness, reuse of update verification without accidental state writes, and frozen-state coverage.

## Phase 7: End-To-End Fixture Creation

Goal: fixtures are created from realistic CometBFT/Cosmos SDK flows and can be regenerated by another implementer.

Implementation tasks:

- Keep `scripts/cometbft-fixture` focused on local-CometBFT update/header fixtures and `scripts/native-ics23-fixture` focused on portable ICS-23 proof conversion.
- Add or extend a sibling e2e fixture generator that starts from real CometBFT headers, validator sets, commits, and SDK store proofs.
- Generate fixtures for:
  - initial client state and consensus state,
  - one or more adjacent updates,
  - 3-validator update,
  - 20-validator update,
  - membership proof for packet commitment,
  - membership proof for acknowledgement,
  - non-membership proof for packet receipt timeout,
  - double-sign misbehaviour,
  - time-monotonicity misbehaviour.
- Include expected hashes/sign bytes/root/path/value/timestamp fields so Solidity tests can assert intermediate values when useful.
- Make fixture generation deterministic and document prerequisites, including the local CometBFT branch for update fixtures and the portable Go/Foundry requirements for proof fixtures.
- Add a `just` recipe such as `just generate-cometbft-fixtures`.

Tests:

- Fixture regeneration produces stable output.
- Solidity tests consume only generated JSON fixtures for proof hot paths.
- Go fixture generator tests verify CometBFT `light.VerifyAdjacent` and SDK proof verification before emitting Solidity JSON.

Validation:

```bash
just check-cometbft-fixtures
just generate-cometbft-fixtures
git diff --check
forge test --config-path foundry-cometbft/foundry.toml -vvv
```

Phase gate:

- Reviewers check reproducibility, absence of hand-crafted proof shortcuts, and whether fixtures would catch a disagreement with CometBFT or SDK proof verification.

## Phase 8: Differential, Fuzz, Gas, And CI

Goal: make regressions hard to introduce and performance visible.

Implementation tasks:

- Add differential tests against Go-generated expected header hashes, validator hashes, vote sign bytes, recovered signers, proof roots, and proof outcomes.
- Add Foundry fuzz tests around timestamp nanos, validator ordering, voting power quorum, proof path/value mutation, and height/revision mutation.
- Add gas report tests for update, membership, non-membership, and misbehaviour paths with realistic fixture sizes.
- Keep portable proof fixture drift checks in CI.
- Wire the focused CometBFT suite into CI or document the exact CI gap for update fixtures while `scripts/cometbft-fixture` depends on a local CometBFT replace.
- Ensure broad repo commands still pass.

Validation:

```bash
forge test --config-path foundry-cometbft/foundry.toml -vvv
forge test --config-path foundry-cometbft/foundry.toml --gas-report
just test-foundry-cometbft
just test-foundry
just build-contracts
git diff --check
```

Phase gate:

- Reviewers check fuzz target quality, gas clarity, CI reproducibility, and whether tests overfit fixture constants.

## Phase 9: Production Readiness Review

Goal: decide whether this is ready to merge as a full native CometBFT light client for the supported target.

Implementation tasks:

- Re-read `docs/adr/native-cometbft-light-client.md` and update the "out of scope" list to remove completed membership, non-membership, and misbehaviour items.
- Keep non-adjacent updates and non-`secp256k1eth` key types explicitly out of scope unless they were intentionally implemented.
- Document gas numbers and any operational assumptions for relayers.
- Document fixture regeneration and local CometBFT branch requirements.
- Prepare PR summary, risk section, and test evidence.

Final validation:

```bash
bun install --frozen-lockfile
forge fmt --check contracts/light-clients/cometbft/**/*.sol test/cometbft/**/*.sol
forge test --config-path foundry-cometbft/foundry.toml -vvv
forge test --config-path foundry-cometbft/foundry.toml --gas-report
just test-foundry-cometbft
just test-foundry
just build-contracts
go -C /Users/gg/code/contrib/cometbft test ./crypto/secp256k1eth ./types
just check-cometbft-fixtures
git diff --check
```

Final review gate:

- Spawn at least three reviewers:
  - correctness/security reviewer,
  - simplicity/maintainability reviewer,
  - test/e2e-fixture reviewer.
- No known high-confidence soundness, complexity, or test-quality finding remains unresolved.
