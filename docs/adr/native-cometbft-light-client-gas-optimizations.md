# Native CometBFT Light Client Gas Optimization Inventory

## Context

This note documents the gas optimization options for the native Solidity CometBFT light client work in this repository.

Relevant repositories and branches:

- Solidity repo: `/Users/gg/code/contrib/solidity-ibc-eureka`
- Native light-client prototype branch: `gjermund/cometbft-lc-poc`
- Message-supplied skipping prototype branch/worktree: `/Users/gg/code/contrib/solidity-ibc-eureka-skip-message`, branch `gjermund/cometbft-skip-message`
- Stored-validator skipping prototype branch/worktree: `/Users/gg/code/contrib/solidity-ibc-eureka-skip-storage`, branch `gjermund/cometbft-skip-storage`
- CometBFT repo: `/Users/gg/code/contrib/cometbft`
- CometBFT branch: `gjermund/moar-ethsecp`
- Relevant CometBFT commit during this analysis: `53171bf261ee80504c9faac62fe06451aca75773`

The current native client relies on the CometBFT `secp256k1eth` key type from the local CometBFT branch. This lets CometBFT validators use Ethereum-recoverable secp256k1 signatures while keeping validator-set hashing compatible with normal CometBFT `ValidatorSet.Hash()` semantics over public keys and voting power.

The gas problem is structural: Solidity is asked to decode large nested messages, reconstruct CometBFT protobuf-style encodings, recompute RFC6962 SHA-256 Merkle roots, derive Ethereum addresses from compressed secp256k1 public keys, and recover commit signatures until quorum.

## Baseline Numbers

The isolated update gas numbers measured for the message-supplied skipping prototype were:

| Update path | Approximate gas |
| --- | ---: |
| 3-validator adjacent update | `279,739` |
| 3-validator skipping update | `332,262` |
| 20-validator adjacent update | `791,470` |
| 20-validator skipping update | `1,227,708` |

Measured ABI-encoded update message sizes were:

| Update path | Calldata bytes |
| --- | ---: |
| 3-validator adjacent update | `2,880` |
| 3-validator skipping update | `3,552` |
| 20-validator adjacent update | `12,128` |
| 20-validator skipping update | `16,608` |

The important deltas are:

| Comparison | Gas delta |
| --- | ---: |
| 3-validator adjacent to 20-validator adjacent | `+511,731` |
| 3-validator skipping to 20-validator skipping | `+895,446` |
| 20-validator adjacent to 20-validator skipping | `+436,238` |

These deltas show that validator-count scaling and the extra trusted-validator-set work in skipping mode dominate the cost.

## Main Cost Drivers

### ABI decode and calldata size

The message-supplied skipping design carries a large nested update message:

- trusted height
- trusted consensus state
- header
- commit
- validator set
- trusted validator set for skipping
- one commit signature slot per validator
- compressed public keys and `y` witnesses

Decoding this structure into memory costs meaningful gas before verification starts.

### Validator-set hash reconstruction

CometBFT validator-set hashes are RFC6962-style SHA-256 Merkle roots over protobuf-encoded `SimpleValidator{pub_key, voting_power}` leaves.

For a 20-validator set:

- 20 leaf SHA-256 calls
- 19 inner SHA-256 calls
- 39 SHA-256 calls per validator set

For a 20-validator skipping update, Solidity hashes both:

- target validator set
- trusted validator set

This is about 78 SHA-256 calls for validator-set hashes alone.

The SHA-256 precompile calls are not the main cost by themselves. The expensive part is the surrounding Solidity work: dynamic `bytes` allocations, `abi.encodePacked`, varint encoding, memory copying, and recursive Merkle construction.

### Header hash reconstruction

The CometBFT header hash covers 14 fields using the same RFC6962-style Merkle construction.

This adds roughly:

- 14 leaf SHA-256 calls
- 13 inner SHA-256 calls
- 27 SHA-256 calls

Again, the raw precompile cost is smaller than the memory and encoding work around it.

### Validator address derivation

The client receives compressed secp256k1 public keys plus a `y` witness. For each validator address derivation, Solidity:

- checks the compressed key shape
- checks the `y` witness is on secp256k1
- checks compressed-key parity
- derives the Ethereum address from `keccak256(x || y)`

This is repeated in several paths today:

- validator ordering checks
- duplicate checks
- commit signer matching
- trusted-overlap matching

Repeated derivation is one of the best local optimization targets.

### Signature verification

Raw `ecrecover` is significant but not the dominant cost.

For the 20-validator skipping fixture, the verifier needs approximately:

- 4 trusted-overlap recoveries to exceed the one-third trust threshold
- 9 target-quorum recoveries to exceed two-thirds voting power

Raw precompile cost is therefore approximately:

```text
13 * 3,000 = 39,000 gas
```

The surrounding work also costs gas: vote sign-byte construction, signature length and low-S checks, validator matching, and voting-power accumulation.

## Solidity-Only Optimizations

These can be implemented without changing the CometBFT branch, the consensus protocol, or the high-level proof model.

### Cache validator addresses per validator set

Impact: high for the current Solidity code.

Risk: low to medium.

The current implementation derives validator addresses repeatedly. A better shape is:

1. Validate each validator public key and `y` witness once.
2. Store derived addresses in an `address[] memory`.
3. Reuse those addresses for ordering, duplicate checks, commit verification, and trusting verification.

This avoids repeated secp256k1 witness checks and repeated `keccak256(x || y)` calls.

Expected benefit:

- meaningful for 20-validator updates
- more meaningful for skipping updates because two validator sets are involved
- likely one of the first optimizations to implement

Correctness requirements:

- address cache must be derived from the same validator entries that are hashed
- duplicate and ordering checks must use cached addresses exactly as before
- tests must mutate `y`, pubkey prefix, validator ordering, and duplicate validators to confirm failures still happen

### Replace nested duplicate checks with sorted-order checks

Impact: medium.

Risk: medium.

If the validator ordering rule guarantees a strict total order by voting power and address, duplicates can be rejected during the single adjacent comparison instead of with an additional nested duplicate scan.

Current logic includes a nested duplicate check. For `n` validators this is O(n^2). If address caching is implemented, this becomes less expensive, but removing the nested scan is still cleaner.

Correctness requirements:

- the ordering rule must make duplicate addresses impossible to pass
- equal voting power must require strictly increasing addresses
- tests must include duplicate validators with the same and different voting power

### Validate commit signatures once

Impact: small to medium.

Risk: low.

In skipping mode, basic commit-signature validation can happen in both target quorum verification and trusted-overlap verification. This can be factored so basic checks run once per commit signature and are reused by both passes.

This saves:

- repeated flag checks
- repeated timestamp nanos checks
- repeated signature length checks

This is not the largest cost, but it is simple.

### Precompute total voting power while validating validators

Impact: small.

Risk: low.

The verifier walks validator arrays multiple times:

- ordering validation
- validator-set hash construction
- total voting power calculation
- signature verification

Some of this can be combined. For example, validation can return:

- derived addresses
- total voting power

The hash still needs the original validator entries, but address derivation and power summation do not need independent passes.

### Use signer indexes for target commit verification

Impact: medium.

Risk: medium.

CometBFT commit signatures are positionally aligned with the validator set in normal commits. The current target verification already relies on the signature index for validator matching. If future message formats allow sparse commits, the message should include validator indexes for each included signature.

This avoids requiring absent signature slots for every validator and reduces calldata.

Correctness requirements:

- indexes must be strictly increasing or duplicate-checked
- each index must be in range
- each signature's validator address must match the indexed validator
- voting power must only be counted once

### Add trusted-validator index hints for skipping verification

Impact: high for large validator sets.

Risk: medium.

The current trusting verification scans the trusted validator set by address for each commit signature until it finds a match. The update message can include an index hint mapping each trusted-overlap signature to its trusted validator index.

Instead of:

```text
for each commit signature:
  for each trusted validator:
    compare addresses
```

the verifier can do:

```text
trustedIndex = hint[i]
expected = trustedValidatorAddresses[trustedIndex]
require(sig.validatorAddress == expected)
```

This changes the trusting check from an O(commit signatures * trusted validators) scan into an O(signatures) check.

Correctness requirements:

- index bounds checks
- duplicate trusted index rejection
- validator address equality check
- recovered signer equality check
- voting power counted only once
- tests for wrong hint, duplicate hint, out-of-range hint, and honest hint

### Use sparse signatures

Impact: high for calldata and decode cost.

Risk: medium to high.

The message currently carries one commit signature slot per validator. The contract only needs enough signatures to exceed the relevant thresholds.

A sparse signature message could include only:

- signatures needed for target quorum
- signatures needed for trusted-overlap verification
- validator indexes for those signatures
- original commit block ID, height, round, and timestamp fields needed to reconstruct sign bytes

This reduces calldata and ABI decode cost. It also avoids looping over absent signatures.

Correctness requirements:

- the sparse message must still represent valid CometBFT commit signatures
- each included signature must bind to the same commit block ID
- the verifier must reject duplicate validator indexes
- voting power thresholds must be computed from the full validator set or a trusted commitment to total voting power

### Optimize vote sign-byte construction

Impact: medium.

Risk: medium to high.

`voteSignBytes` builds protobuf-compatible canonical vote bytes using dynamic `bytes` and `abi.encodePacked`. This happens once per recovered signature.

Possible optimizations:

- precompute common vote fields shared by all validators
- construct only the timestamp-specific portion per signature
- use pre-sized buffers instead of nested `abi.encodePacked`
- avoid constructing intermediate `bytes` values

This is correctness-sensitive because the bytes must match CometBFT exactly.

Tests must include differential vectors from CometBFT for every supported field combination.

### Optimize protobuf-style field encoders

Impact: medium.

Risk: medium to high.

The current `CometBFTProto` helpers use many small dynamic byte arrays:

- `encodeVarint`
- `encodeFixed64`
- `encodeTimestamp`
- `encodeBlockID`
- `encodeSimpleValidator`
- wrapper functions for fields

Gas can be reduced by writing into pre-sized memory buffers or by using assembly for hot encoders.

This should only be done after differential tests are strong, because encoding bugs are consensus bugs.

### Optimize RFC6962 Merkle hashing

Impact: medium.

Risk: medium.

The current Merkle implementation accepts `bytes[] memory` leaves and recursively hashes ranges. This creates memory overhead.

Possible optimizations:

- hash leaves into a `bytes32[]` first
- iteratively combine levels instead of recursive calls
- avoid keeping all encoded leaves alive if streaming level construction is possible
- use pre-sized buffers for `0x00 || leaf` and `0x01 || left || right`

The hash result must remain byte-for-byte identical to CometBFT's `merkle.HashFromByteSlices`.

### Avoid OpenZeppelin ECDSA wrapper overhead

Impact: low.

Risk: low to medium.

The client already validates signature length, `v`, and low-S. It could call `ecrecover` directly instead of going through OpenZeppelin's `ECDSA.tryRecover`.

Expected savings are small compared with encoding and validator-set work.

### Use unchecked arithmetic in bounded loops

Impact: low.

Risk: low if carefully bounded.

Several loops can use `unchecked { ++i; }` after bounds are clear. This saves small amounts of gas.

This should be applied after larger structural optimizations, not before.

### Custom errors and revert data minimization

Impact: low for successful updates.

Risk: low.

The client already uses custom errors. Further minimizing revert payloads mostly helps failing paths, not successful update gas.

## ABI And Message-Shape Optimizations

These change the Solidity update ABI but do not require CometBFT consensus changes.

### Separate adjacent and skipping update messages

Impact: medium.

Risk: low to medium.

Adjacent updates do not need trusted validators. Skipping updates do. A split ABI can make the adjacent path smaller and simpler:

- `MsgAdjacentUpdateClient`
- `MsgSkippingUpdateClient`

This avoids optional fields and keeps each path easier to audit.

### Remove redundant trusted consensus state from messages when possible

Impact: small to medium.

Risk: medium.

If the trusted consensus state is already stored on-chain, the message may not need to carry it. The contract can load it by trusted height.

This saves calldata and decode cost, but the interface must still preserve replay and misbehaviour semantics.

### Include verifier hints generated off-chain

Impact: medium.

Risk: medium.

Hints can reduce on-chain searching without trusting the hints:

- target signer indexes
- trusted signer indexes
- sorted signer order
- expected voting-power prefix sums

The contract must verify every hint against committed data before using it.

### Use compact fixed-width structs where safe

Impact: small to medium.

Risk: medium.

Some values can be represented more compactly than the current ABI shape:

- `uint64` heights
- `uint32` rounds and nanos
- `uint64` timestamps
- fixed-size compressed pubkeys as `bytes33` is not available in Solidity, but `bytes` can sometimes be replaced by `bytes32 x` plus `uint8 prefix`
- signatures as `bytes32 r`, `bytes32 s`, `uint8 v`

This reduces dynamic decode overhead but makes fixture generation and ABI compatibility more custom.

### Pass public key coordinates instead of compressed pubkey plus y witness

Impact: small to medium.

Risk: medium.

The validator-set hash commits to the compressed public key, so the compressed key still must be present or reconstructable. But the verifier could receive:

- compressed prefix
- x coordinate
- y coordinate

instead of a dynamic `bytes` pubkey plus y witness.

This avoids dynamic `bytes` decoding for pubkeys and simplifies address derivation. The contract must still reconstruct the compressed pubkey bytes for validator-set hashing.

## Storage-Based Optimizations

These trade update cost, storage cost, and operational complexity.

### Store validator sets on-chain

Impact: mixed.

Risk: high.

Instead of carrying validator sets in every update, the contract can store validator sets or validator-set commitments.

Potential benefits:

- smaller update messages after the validator set is known
- less repeated validator hash reconstruction
- skipping updates may avoid carrying the full trusted set

Costs:

- expensive initial storage writes
- validator-set change management
- pruning/retention policy
- higher contract state footprint
- more complex replay and revision semantics

The stored-validator prototype exists, but the message-supplied version was preferred because it is simpler and has fewer lifecycle risks.

### Store derived validator addresses

Impact: medium.

Risk: medium.

If validator sets are stored, derived Ethereum addresses can also be stored. This avoids repeated witness validation and address derivation.

This is only attractive if validator-set storage is already accepted.

### Store total voting power with validator-set commitments

Impact: medium.

Risk: medium.

Total voting power is needed for quorum calculations. Storing it alongside a validator-set commitment avoids recomputing it from the full set.

This requires a trusted storage update path that binds total voting power to the validator set.

## CometBFT Export And Tooling Optimizations

These do not change CometBFT consensus. They improve the proof/update data that relayers or fixture generators submit to Solidity.

### Add a CometBFT Ethereum update builder

Impact: medium to high.

Risk: low.

CometBFT or a companion tool can export an Ethereum-optimized update object:

- canonical validator order
- full target validator set
- full trusted validator set only when skipping
- minimal required signatures
- signer indexes
- trusted signer index hints
- `y` witnesses for secp256k1eth validators
- CometBFT differential vectors for hashes and sign bytes

This would not change consensus. It would reduce fixture ambiguity and make Solidity tests easier to keep in sync with CometBFT.

### Export sparse commits for light clients

Impact: medium.

Risk: low to medium.

CometBFT RPC or tooling can expose only the commit signatures needed for a given trust threshold and target quorum, plus validator indexes.

The full commit remains the consensus object. The sparse representation is only a light-client proof format.

### Export trusted-overlap witness data

Impact: medium.

Risk: low.

For skipping updates, tooling can compute which signatures satisfy the trust-level threshold against the trusted validator set and include:

- commit signature index
- trusted validator index
- trusted voting power

Solidity still verifies all data, but avoids searching.

### Emit EVM differential test vectors

Impact: indirect but important.

Risk: low.

The CometBFT branch should emit stable vectors for:

- validator bytes
- validator-set hash
- header hash
- vote sign bytes
- recovered Ethereum signer
- commit quorum and trust-level decisions

This makes aggressive Solidity optimizations safer.

## CometBFT Consensus-Level Optimizations

These require changing what CometBFT commits to or what validators sign. They are the largest potential gas wins, but they are protocol changes.

### EVM-friendly validator-set commitment

Impact: very high.

Risk: high.

Today the header commits to CometBFT's normal validator-set hash over protobuf-encoded validators. Solidity must receive and hash the full set.

A more EVM-friendly commitment could use leaves such as:

```text
leaf = hash(index, ethereumAddress, votingPower, pubkeyType)
```

The commitment should also bind:

- total voting power
- validator count
- revision or validator-set version

Then an update could include only the signing validators plus Merkle proofs, instead of the full validator set.

This is the biggest possible gas improvement for large validator sets.

### Merkle-sum validator tree

Impact: very high.

Risk: high.

A Merkle-sum tree can commit to both membership and voting power. Each proof can show:

- validator membership
- validator voting power
- aggregate total voting power

This would let Solidity verify quorum from a subset of signer proofs without receiving the whole validator set.

This is a deep consensus/data-structure change but is the most directly aligned with on-chain verification.

### Commit Ethereum addresses in validator hashes

Impact: medium.

Risk: high.

If CometBFT validator-set hashes committed to Ethereum addresses directly, Solidity would not need to derive addresses from compressed public keys for every validator.

This would reduce address-derivation cost but would not by itself remove the need to process the full validator set.

The current CometBFT branch intentionally keeps validator hashes on public keys to preserve normal CometBFT semantics.

### Commit uncompressed public keys or y coordinates

Impact: medium.

Risk: high.

If validator data committed to the full secp256k1 point, Solidity would not need a separate `y` witness or parity validation.

This helps, but it increases validator data size and changes the consensus commitment.

### Change vote sign bytes to share one digest across validators

Impact: medium.

Risk: high.

CometBFT precommit sign bytes include validator-specific data through each commit signature's timestamp and validator index/address path. If validators signed a common block digest, Solidity could construct one digest and recover all signers against it.

This is a major signing-spec change and would affect consensus safety, evidence, and compatibility.

### Aggregate signatures

Impact: very high.

Risk: very high.

An aggregate signature scheme could reduce signature verification from many `ecrecover` calls to one aggregate verification.

This is not obviously attractive on Ethereum L1 unless the aggregate scheme has an efficient precompile. BLS12-381 may be better on some chains than others, but this would be a large CometBFT validator-key and consensus-signing change.

## Optimizations That Are Probably Not Worth Prioritizing

### Raw ecrecover micro-optimization first

Raw `ecrecover` cost is not the main problem. For the 20-validator skipping fixture, raw recovery is approximately tens of thousands of gas, while the total update is over one million gas.

Direct `ecrecover` may still be worth doing eventually, but it should not be the first optimization.

### Replacing SHA-256 precompile usage

CometBFT uses SHA-256 for these hashes. Solidity must match CometBFT. The SHA-256 precompile itself is relatively cheap. The surrounding memory and encoding work is the target, not the hash primitive.

### Storing every possible intermediate value

Caching can help, but pushing too much into contract storage risks making the design more expensive and harder to operate than the message-supplied design.

Storage should be used only when there is a clear lifecycle model for validator-set updates, pruning, replay handling, and revision boundaries.

## Recommended Implementation Order

1. Add differential gas tests for adjacent and skipping updates at 3, 20, and larger validator counts.
2. Cache validator addresses per validator set in memory.
3. Remove or replace O(n^2) duplicate scans if strict ordering already proves uniqueness.
4. Factor basic commit-signature validation so skipping updates do not repeat it.
5. Add trusted-validator index hints for skipping updates.
6. Consider sparse signatures with validator indexes.
7. Optimize vote sign-byte construction with differential vectors.
8. Optimize validator/header protobuf encoding and Merkle hashing.
9. Only then consider storage-based validator-set caching.
10. If the chain protocol can change, design an EVM-friendly validator-set commitment or Merkle-sum tree.

## Required Test Coverage For Gas Optimizations

Every optimization should preserve or add tests for:

- valid adjacent update
- valid skipping update
- wrong trusted revision
- expired trusted state
- future header
- wrong validator-set hash
- wrong trusted validator-set hash
- wrong header hash
- wrong commit height
- invalid commit block ID
- insufficient target voting power
- insufficient trusted voting power
- duplicate target signer
- duplicate trusted signer
- wrong signer index hint
- duplicate signer index hint
- out-of-range signer index hint
- malformed compressed public key
- invalid `y` witness
- invalid signature length
- invalid signature `v`
- high-S signature
- timestamp nanos out of range
- CometBFT differential vectors for validator-set hash, header hash, vote sign bytes, and recovered signer

Gas reports should be kept for:

- 3-validator adjacent update
- 3-validator skipping update
- 20-validator adjacent update
- 20-validator skipping update
- at least one larger validator set if fixture generation supports it

## Summary

The best near-term gas work is not a crypto primitive swap. It is reducing repeated local work and reducing message size:

1. Cache validator addresses.
2. Avoid O(n^2) trusted-validator scans with verified index hints.
3. Use sparse signatures where safe.
4. Reduce dynamic protobuf and Merkle memory churn.

The biggest long-term gas win requires a CometBFT-level commitment better suited for on-chain verification: an indexed validator commitment that binds Ethereum address, voting power, and total voting power, ideally with Merkle or Merkle-sum proofs. That would let Solidity verify only the signing subset instead of receiving and hashing the full validator set on every update.
