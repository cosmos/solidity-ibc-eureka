## Solidity IBC Attestor Light Client — Design

This document specifies how the Solidity light client for IBC Attestations will work. It mirrors the high-level semantics of the CosmWasm 08-wasm attestor client while adapting details to EVM constraints and `ILightClient` in this repository. It focuses on behavior and interfaces, not implementation.

### Goals
- Provide an IBC light client that trusts an off-chain m-of-n attestor set for state updates and packet membership verification.
- Be compatible with the existing attestor/aggregator services and the Rust core logic where possible.
- Integrate with `ICS26Router` via `ILightClient`.
- Support membership checks for packet commitments attested as a list of commitments.

### Non-goals (initial scope)
- Non-membership verification.
- Misbehaviour evidence processing and freezing (placeholder/TODO only).
- Time validity windows (no clock-drift checks beyond stored timestamps).
- Upgrades (unsupported).

## System Overview

- Off-chain attestors sign opaque attestation data. The aggregator collects signatures and forms headers/proofs.
- On-chain, the light client:
  - Verifies m-of-n signatures over the attested data using the configured attestor set.
  - Updates a minimal consensus state per height: `(height, timestamp)`.
  - Verifies membership by checking that a provided packet commitment is contained in an attested list for a given height.

## Data Model

- ClientState
  - `attestorAddresses: address[]` — fixed set of attestor identities (addresses favored on EVM for verification efficiency).
  - `minRequiredSigs: uint8` — quorum threshold.
  - `latestHeight: uint64` — highest known height.
  - `isFrozen: bool` — reserved for misbehaviour (not used until TODO is implemented).

- Consensus state storage
  - `mapping(uint64 => uint64) consensusTimestampAtHeight` — maps `height -> timestampSeconds`.
  - Storing height and timestamp is sufficient. No commitment root is stored.

## Hashing and Signatures

- Hashing: SHA-256 of the exact `attestationData` bytes (same as Rust core). On-chain we use Solidity `sha256(bytes)` to produce the 32-byte digest used for signature checks.
- Signatures: secp256k1 ECDSA signatures with 65 bytes `(r || s || v)` per signature.
  - We use OpenZeppelin `ECDSA.recover(digest, signature)` for recovery and low-s malleability protection. `ECDSA.recover` accepts `v` in either 27/28 or 0/1 form; we will accept both.
  - Each recovered signer address must belong to the fixed attestor set, and each must be unique within the verification set. Enforce `minRequiredSigs` quorum.

## Encodings and Wire Compatibility

- Header/proof payloads passed to the Solidity client are ABI-encoded for gas efficiency and simplicity.
- Attested data must be decoded on-chain, so JSON/Serde payloads used by Rust are not practical on EVM.
- Attested data format on EVM:
  - `attestationData = abi.encode(packetCommitments: bytes32[])` where each element is a single packet commitment `bytes32`.
  - This enables efficient on-chain decoding and membership checks.
- TODO: Update the CosmWasm attestor client to adopt the same ABI-equivalent encoding so both clients are wire-compatible at the attested-data level.

## External Interface (behavioral)

The client implements `ILightClient` with the following behaviors.

### updateClient(updateMsg)

Purpose: Validate an aggregated header signed by the attestor set and update the consensus state at the new height.

Input (ABI-encoded inside `updateMsg`, conceptual):
- `newHeight: uint64`
- `timestamp: uint64` (seconds)
- `attestationData: bytes` (ABI-encoded `bytes32[]` of packet commitments, see Membership section)
- `signatures: bytes[]` (each 65-byte `(r||s||v)`) — no claimed signer addresses required

Verification:
- Compute `digest = sha256(attestationData)`.
- For each `signature65` use `ECDSA.recover(digest, signature65)` to obtain `signer`.
- Require each `signer` to be in `attestorAddresses` and all recovered signers to be unique.
- Enforce `numUniqueValidSigners >= minRequiredSigs`.

State transition:
- If `newHeight < latestHeight`: revert (out-of-order updates not allowed initially).
- If `newHeight == latestHeight`:
  - If `timestamp == consensusTimestampAtHeight[newHeight]`: return `UpdateResult.NoOp`.
  - Else: conflicting timestamp for same height. Return `UpdateResult.NoOp` and emit a signal (event) OR revert. Misbehaviour handling is a TODO; freezing is not performed in this version.
- If `newHeight > latestHeight`:
  - Set `consensusTimestampAtHeight[newHeight] = timestamp`.
  - Set `latestHeight = newHeight`.
  - Return `UpdateResult.Update`.

Access control:
- Gated by `PROOF_SUBMITTER_ROLE` (same semantics as `SP1ICS07Tendermint`):
  - If `address(0)` has the role, anyone can submit.
  - Otherwise, caller must have the role (e.g., `ICS26Router`).

Notes:
- No time-window or clock-drift validation is performed. Only monotonicity by height is enforced (and equality rule for same-height repeats).

### verifyMembership(MsgVerifyMembership)

Purpose: Verify that a single packet commitment value exists within an attested list at a given height and return the trusted timestamp for that height.

Usage:
- `value` is a single packet commitment `bytes`.
- `attested list` is passed inside the `proof` and must be the exact data signed by the attestors.

Input mapping (from `ILightClientMsgs.MsgVerifyMembership`):
- `proof: bytes` — ABI-encoded struct:
  - `attestationData: bytes` — `abi.encode(packetCommitments: bytes[])`.
  - `signatures: bytes[]` — 65-byte `(r||s||v)` per signature.
- `proofHeight: Height` — the attestation height.
- `path: bytes[]` — ignored by this client.
- `value: bytes` — the packet commitment to check for membership.

Verification:
- Require `consensusTimestampAtHeight[proofHeight.revisionHeight]` to exist.
- Compute `digest = sha256(attestationData)` and verify signatures as in `updateClient` via `ECDSA.recover`.
- Decode `attestationData` into `bytes32[] packetCommitments` via `abi.decode(attestationData, (bytes32[]))`.
- Require `value` to match exactly one element of `packetCommitments` (byte equality).

Return:
- The trusted timestamp (in seconds) stored for `proofHeight.revisionHeight`.

Access control:
- Gated by `PROOF_SUBMITTER_ROLE`.

### verifyNonMembership(...)

Out of scope for this version. The function MUST revert with a clear "feature not supported" error.

### misbehaviour(...)

Out of scope for this version. The function exists but MUST revert with a clear TODO note to implement evidence verification and freezing.

### upgradeClient(...)

Unsupported. MUST revert.

## Roles and Permissions

- `DEFAULT_ADMIN_ROLE`: manages roles.
- `PROOF_SUBMITTER_ROLE`: required to call `updateClient` and (non-)membership queries. Grant this to `ICS26Router` when used in IBC. If granted to `address(0)`, anyone may submit proofs.

## Time and Ordering

- No validity window or clock-drift checks are enforced. The timestamp stored in consensus state is treated as an attested fact tied to the height.
- Height progression must be monotonic. Replays at the same height with identical timestamp are idempotent (`NoOp`). Conflicts at same height are not resolved in this version (see Misbehaviour TODO).

## Replay and Domain Separation

This version does not embed domain separation fields in the signed attestation bytes. This introduces potential replay risk across different clients or contexts if the same `attestationData` can be reused elsewhere.

Recommended future hardening (non-breaking if added to the signed payload definition):
- Include a typed domain in the attested bytes such as one or more of:
  - counterparty chain-id, IBC `client-id`, this light client contract address, local chain-id, and a protocol tag/version.
- Document exact encoding and update both Solidity and CosmWasm clients accordingly.

## Security Considerations

- Threshold enforcement: enforce uniqueness of recovered signer addresses and minimum quorum.
- Signature malleability: OpenZeppelin `ECDSA` enforces low-`s` and rejects malleable signatures.
- Attestor set management: fixed in client state for initial scope; rotation requires a future governance-controlled upgrade path.
- Large attested lists: no explicit limits are imposed in this design; callers must size proofs responsibly to avoid out-of-gas.

## ICS-26 Integration

- The client implements `ILightClient`. The router invokes:
  - `updateClient` to advance consensus state.
  - `verifyMembership` during proof verification flows (returns the counterparty timestamp in seconds).
  - Other interface methods revert as described above.

## Open TODOs / Future Work
- Implement misbehaviour detection and freezing (conflicting headers for the same height, etc.).
- Standardize `attestationData` encoding across platforms (update CosmWasm client to use the ABI-equivalent format defined here).
- Consider adding domain separation to the signed payload.
- Implement monotonic increase check
- Consider non-membership proofs.
- Consider client upgrades for attestor set rotation and quorum changes.
- Optional: proof caching within a transaction if needed for multi-membership checks.


