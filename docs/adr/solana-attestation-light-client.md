# ADR: Solana Attestation Light Client

## Status

Implemented

## Context

IBC (Inter-Blockchain Communication) requires light clients to verify state from counterparty chains. The Solidity attestation light client already exists for Ethereum, providing a trust-minimized approach where a set of attestors sign state commitments.

The goal of the Solana implementation is to **mirror the Solidity attestation light client** as closely as possible while adapting to Solana's programming model. This ensures:
- Consistent behavior across chains
- Shared attestor infrastructure
- Predictable verification semantics

This ADR describes the design decisions for implementing the attestation light client on Solana and documents where platform differences required divergent approaches.

## Decision

### Core Design

The attestation light client trusts an m-of-n set of attestors to honestly report state from the counterparty chain. Attestors sign:
- **State attestations**: height and timestamp for client updates
- **Packet attestations**: commitment paths and values for membership proofs

### Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Counterparty   │────▶│    Attestors    │────▶│  Solana Light   │
│     Chain       │     │   (off-chain)   │     │     Client      │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │                       │
        │  State/Packets        │  Signed Attestations  │  Verify & Store
        └───────────────────────┴───────────────────────┘
```

### Key Design Decisions

#### 1. Signature Scheme: ECDSA with secp256k1

**Decision**: Use Ethereum-compatible ECDSA signatures (secp256k1).

**Rationale**:
- Attestors are Ethereum addresses (20-byte)
- Reuses existing Ethereum key infrastructure
- Solana provides native `secp256k1_recover` syscall
- Compatible with Solidity implementation

**Tradeoff**: Each signature is 65 bytes. BLS signatures would allow aggregation but require different infrastructure.

#### 2. Message Hashing: SHA256

**Decision**: Hash attestation data with SHA256 before signing.

**Rationale**:
- Matches the signing approach used by attestors
- Consistent with Solidity implementation
- Solana has efficient SHA256 support

**Alternative Considered**: Keccak256 (Ethereum's native hash). Rejected because attestors use SHA256.

#### 3. Proof Serialization: Borsh

**Decision**: Use Borsh for MembershipProof serialization.

**Rationale**:
- Borsh is Solana's native serialization format
- ~2.5x more compact than JSON
- Critical for fitting proofs within transaction size limits

**Impact**: With Borsh, each signature adds ~69 bytes vs ~165 bytes with JSON.

#### 4. Consensus State Storage: Per-Height PDAs

**Decision**: Store each consensus state in a separate PDA account derived from (client_id, height).

```rust
seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &height.to_le_bytes()]
```

**Rationale**:
- Natural fit for Solana's account model
- Enables parallel access to different heights
- Account existence serves as implicit "height trusted" check

**Tradeoff**: Creates many small accounts. Alternative would be a single account with all heights, but this limits scalability and complicates updates.

#### 5. Duplicate Submission Prevention: Init Constraint

**Decision**: Use Anchor's `init` constraint for consensus state accounts, preventing updates to existing heights.

**Rationale**:
- Simple and secure - first valid submission wins
- No explicit misbehavior detection needed
- Matches Solana's "create once" account pattern

**Difference from Solidity**: Solidity allows resubmission and freezes on conflicting timestamps. Solana prevents duplicates entirely.

**Security Implication**: Both approaches are secure. Solana's approach is simpler but doesn't provide explicit misbehavior reporting.

#### 6. Access Control: External Program

**Decision**: Delegate access control to a separate `access_manager` program.

**Rationale**:
- Separation of concerns
- Reusable across IBC programs (router, light clients)
- Flexible role management

**Alternative Considered**: Inline access control. Rejected for code reuse reasons.

### Transaction Size Constraints

Solana's 1232-byte transaction limit is the primary constraint:

| Component | Size |
|-----------|------|
| Transaction overhead | ~200 bytes |
| Instruction data (base) | ~300 bytes |
| Per-signature (Borsh) | ~69 bytes |
| Account references | ~32 bytes each |

**Tested Limit**: 11 attestor signatures for `update_client` without chunking.

This was empirically verified through e2e testing:
- 10 signatures: ✅ passes reliably
- 11 signatures: ✅ passes (maximum tested)
- 12+ signatures: ❌ exceeds transaction size limit

The limit applies to all signature-verifying instructions (`update_client`, `verify_membership`, `verify_non_membership`).

**Optimizations Applied**:
1. **Borsh serialization**: Reduced per-signature overhead from ~165 bytes (JSON) to ~69 bytes
2. **Address Lookup Tables (ALT)**: Reduces account reference size from 32 bytes to 1 byte index

**Future Options** (not implemented):
1. Signature chunking - upload signatures in multiple transactions (pattern exists in ICS07 Tendermint LC)
2. BLS aggregation - single aggregated signature regardless of attestor count

### Verification Flow

```
verify_membership(height, path, value, proof):
    1. Validate inputs (path.len == 1, value not empty)
    2. Check client not frozen
    3. Deserialize proof (Borsh)
    4. Decode attestation (ABI)
    5. Verify height matches consensus state
    6. Verify signatures (≥ min_required_sigs, no duplicates, all from trusted set)
    7. Hash path with keccak256
    8. Find matching packet commitment
    9. Compare commitment with provided value
    10. Return timestamp via set_return_data
```

## Consequences

### Positive

1. **Solidity Parity**: Behavior matches the Solidity implementation
2. **Shared Infrastructure**: Reuses same attestor set and signing scheme
3. **Predictable Semantics**: Same verification logic across chains
4. **Fast Verification**: Signature verification is efficient on Solana

### Negative

1. **Signature Limit**: 11 signatures per transaction (Solana-specific constraint)
2. **No Misbehavior Freezing**: Unlike Solidity, doesn't freeze on conflicting attestations (architectural difference)
3. **Account Proliferation**: Creates many small accounts for consensus states

### Risks

1. **Attestor Collusion**: If ≥m attestors collude, they can attest false state
2. **Key Compromise**: Compromised attestor keys reduce security threshold
3. **Liveness**: If <m attestors are available, client cannot be updated

## Alternatives Considered

### 1. BLS Signature Aggregation

**Description**: Use BLS signatures that can be aggregated into a single signature.

**Pros**:
- Single ~96 byte signature regardless of attestor count
- No transaction size limit on attestor count

**Cons**:
- Different key infrastructure (not Ethereum compatible)
- More complex cryptography
- Solana doesn't have native BLS support (would need precompile or program)

**Decision**: Deferred. Can be added later if needed.

### 2. Signature Chunking

**Description**: Upload signatures in chunks across multiple transactions, verify all in final transaction.

**Pros**:
- Supports unlimited attestors
- Pattern exists in ICS07 Tendermint LC

**Cons**:
- Multiple transactions required
- Temporary PDA accounts for chunks
- More complex relayer logic

**Decision**: Deferred. Current 11-signature limit is sufficient for initial deployment.

### 3. Optimistic Verification

**Description**: Accept attestations optimistically, allow challenge period.

**Pros**:
- Lower per-transaction cost
- Can support more attestors

**Cons**:
- Adds latency (challenge period)
- Complex dispute resolution
- Different security model

**Decision**: Rejected. Immediate verification is simpler and matches Solidity behavior.

### 4. Threshold Signatures (TSS)

**Description**: Attestors collaboratively produce a single signature via MPC.

**Pros**:
- Single signature regardless of threshold
- No transaction size concerns

**Cons**:
- Complex key generation ceremony
- Online coordination required for signing
- Different infrastructure than individual ECDSA

**Decision**: Rejected. Too much infrastructure complexity.

## Solidity Comparison

This section details the differences between Solana and Solidity implementations.

### Feature Parity

| Feature | Solidity | Solana |
|---------|----------|--------|
| Client initialization | ✅ | ✅ |
| Client state update | ✅ | ✅ |
| Membership verification | ✅ | ✅ |
| Non-membership verification | ✅ | ✅ |
| Frozen client check | ✅ | ✅ |
| Path length validation (= 1) | ✅ | ✅ |
| Height mismatch check | ✅ | ✅ |
| Height/timestamp > 0 validation | ✅ | ✅ |
| Duplicate signature detection | ✅ | ✅ |
| Signature quorum enforcement | ✅ | ✅ |
| Timestamp return from verify | ✅ | ✅ |
| Access control (roles) | ✅ | ✅ |

### Implementation Differences

#### Misbehavior Detection

**Solidity:**
- Allows resubmitting updates to the same height
- If same timestamp → returns `NoOp`
- If different timestamp → freezes client, returns `Misbehaviour`

**Solana:**
- Uses Anchor's `init` constraint for consensus state accounts
- Account creation fails if consensus state at that height already exists
- First submission wins (immutable)

Both approaches are secure but handle the edge case differently.

#### Consensus State Storage

**Solidity:**
```solidity
mapping(uint64 height => uint64 timestampSeconds) private _consensusTimestampAtHeight;
```

**Solana:**
```rust
#[account(
    seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &height.to_le_bytes()],
    bump
)]
pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
```

#### Signature Verification

**Solidity:**
```solidity
address recovered = ECDSA.recover(digest, signature);
```
- Uses OpenZeppelin's ECDSA library

**Solana:**
```rust
let message_hash: [u8; 32] = Sha256::digest(message).into();
secp256k1_recover(&message_hash, recovery_id, &sig_bytes)
```
- Uses Solana's native syscall
- Manual recovery ID normalization (27/28 → 0/1)

#### Return Values

**Solidity:**
```solidity
function verifyMembership(...) external view returns (uint256) {
    return uint256(ts);
}
```

**Solana:**
```rust
let timestamp_bytes = consensus_state.timestamp.to_le_bytes();
set_return_data(&timestamp_bytes);
```

#### Access Control

| Aspect | Solidity | Solana |
|--------|----------|--------|
| Implementation | OpenZeppelin AccessControl | Separate `access_manager` program |
| Role | `PROOF_SUBMITTER_ROLE` | `PROOF_SUBMITTER_ROLE` (ID=8) |
| Optional | If `roleManager == address(0)`, anyone can submit | Always requires role |

### Error Mapping

| Error Type | Solidity | Solana |
|------------|----------|--------|
| Client frozen | `FrozenClientState()` | `ClientFrozen` |
| Invalid quorum | `BadQuorum(uint8, uint256)` | `InvalidMinRequiredSigs` |
| No attestors | `NoAttestors()` | `InvalidAttestorAddresses` |
| Empty signatures | `EmptySignatures()` | `NoSignatures` |
| Too few signatures | `ThresholdNotMet(...)` | `TooFewSignatures` |
| Duplicate signer | `DuplicateSigner(address)` | `DuplicateSignature` |
| Unknown signer | `UnknownSigner(address)` | `UnknownAddressRecovered` |
| Invalid signature | `SignatureInvalid(bytes)` | `InvalidSignature` |
| Height mismatch | `HeightMismatch(...)` | `HeightMismatch` |
| Invalid state | `InvalidState(...)` | `InvalidState` |
| Path not found | `NotMember()` | `PathNotFound` |
| Invalid path length | `InvalidPathLength(...)` | `InvalidPathLength` |
| Empty value | `EmptyValue()` | `EmptyValue` |

### Platform-Specific Constraints

**Solana:**
- Maximum transaction size: 1232 bytes
- Tested limit: 11 attestor signatures without chunking
- Account rent required for consensus state PDAs

**Solidity:**
- No practical limit on signatures (gas cost scales linearly)
- No storage rent

## References

- [IBC Specification](https://github.com/cosmos/ibc)
- [Solana Program Documentation](https://docs.solana.com/developing/programming-model/overview)
- [Anchor Framework](https://www.anchor-lang.com/)
- [Solidity Attestation Light Client](../../contracts/light-clients/attestation/)
