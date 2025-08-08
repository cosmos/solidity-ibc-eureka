# ADR: Solana IBC Storage Architecture

**Status**: Proposed
**Date**: 2025-06-17

## Context

The Solana IBC implementation requires efficient storage mechanisms for both light client consensus states and packet lifecycle data. Unlike EVM chains where storage is managed within contract state, Solana's account model requires explicit decisions about data distribution across Program Derived Addresses (PDAs). Solana programs cannot use standard Rust collections like `BTreeMap` or `BTreeSet`, and the idiomatic approach is to use PDAs as a key-value store.

## Decision

We will implement a PDA-based storage architecture with pruning strategies for both consensus states and packet commitments, maximizing rent reclamation.

### Key Insight: Everything is Temporary

Both consensus states and packets have limited useful lifetimes:
- **Consensus states**: Only recent states needed for active packet verification
- **Packets**: Only needed from send → acknowledge/timeout

This enables aggressive pruning with rent reclamation for both.

### Storage Strategy Comparison

**Consensus States:**
- **Storage**: Rolling window of 100 most recent states per client
- **Lifetime**: Hours to days
- **Pruning**: Close old PDAs when window slides
- **Cost**: ~0.2 SOL per client (constant, not annual)

**Packets:**
- **Storage**: Bitmap for status + PDAs for commitments
- **Lifetime**: Minutes to hours
- **Pruning**: Close PDAs on finalization
- **Cost**: ~0.002 SOL per active packet (temporary)

**Cost Example (1000 packets/day, 1000 consensus states/day):**
```
Traditional (no pruning):
- Consensus: 1000/day × 365 = 365,000 PDAs × 0.002 = 730 SOL/year
- Packets: 1000/day × 365 = 365,000 PDAs × 0.002 = 730 SOL/year
- Total: 1,460 SOL/year

With pruning:
- Consensus: 100 PDAs × 0.002 × 10 clients = 2 SOL (one-time)
- Packets: ~100 active × 0.002 = 0.2 SOL (rotating)
- Total: 2.2 SOL constant + transaction fees

Savings: 664x reduction
```

### 1. Tendermint Light Client Storage (Prunable PDAs)

**Rationale**: IBC verification only requires recent consensus states. Older states can be pruned.

**Structure:**
- Client state PDA: `["client", client_id]` - tracks latest and earliest heights
- Consensus state PDAs: `["consensus_state", client_id, height_bytes]` - rolling window
- Window size: 100 consensus states (configurable)

**Pruning Logic:**
```
When adding consensus state at height H:
1. Create PDA for height H
2. Update client state (latest_height = H)
3. If window full (>100 states):
   - Close PDA for height (H - 100)
   - Update earliest_height in client state
   - Reclaim ~0.002 SOL rent
```

**Why 100 States is Appropriate:**
- High-frequency chains update every ~86 seconds (1000/day)
- 100 states × 86 seconds = ~2.4 hours of history
- Sufficient for packet finalization even with delays
- Balances storage cost vs operational safety

### 2. Packet Storage (Bitmap + Temporary PDAs)

**Rationale**: Combine fast lookups via bitmap with rent-efficient PDA storage.

**Router State Structure:**
- Bitmap tracks packet status (1 bit per packet)
- Window size: 10,000 packets
- Router account size: ~2KB

**Packet Commitment PDAs:**
- Seeds: `["packet_commitment", channel_id, sequence_bytes]`
- Created on send, closed on acknowledge/timeout
- Rent (~0.002 SOL) fully reclaimed

**State Transitions:**
```
Send: Set bit → Create PDA → Emit event
Acknowledge: Clear bit → Close PDA → Reclaim rent
Timeout: Verify → Clear bit → Close PDA → Reclaim rent
```

### 3. Sequence Management

**SequenceLock PDA:**
- Seeds: `["sequence_lock", channel_id]`
- Ensures sequence uniqueness
- Serializes packet sends via Solana runtime

### 4. Non-Membership Proofs (Destination Chain)

**Rationale**: Prove packet was NOT received for timeout processing.

**Approach:**
- Bitmap in router tracks received packets
- Periodic merkle commitments of bitmap chunks (every 100 packets)
- Commitment PDAs: `["recv_commitment", channel_id, chunk_start_bytes]`

**Pruning:**
- Commitments can be closed after IBC challenge period
- Typically 1-2 days retention sufficient

### 5. Off-chain Archival System

**Rationale**: Historical packet data needed for debugging/compliance but not for protocol operation

**Archive Process**:
1. Monitor window for sliding conditions
2. Before sliding, collect all packet data from events
3. Extract active packet sequences that will move to overflow
4. Compute merkle root of archived packets
5. Upload bundle to off-chain storage (IPFS/Arweave/S3)
6. Store reference on-chain (optional)

**Archive Bundle Structure**:
```
{
  window_start: u64,
  window_end: u64,
  packets: Vec<PacketData>,        // From events
  merkle_root: [u8; 32],           // For verification
  active_sequences: Vec<u64>,      // Still in-flight packets
  archived_at: i64,
}
```

**On-chain Reference (Optional)**:
- Seeds: `["archive", channel_id, archive_index]`
- Stores: sequence range, merkle root, timestamp
- Can be pruned after compliance period

**Verification Process**:
- Retrieve bundle from off-chain storage
- Verify merkle root matches on-chain reference
- Extract historical packet data
- Note: Relies on indexer/storage availability, not consensus

**Why Optional**:
- Active packets have PDAs with commitments
- Events provide packet data during operation
- Only needed for historical analysis after finalization
- Not required for protocol security

## Storage Lifecycle

### Consensus State Lifecycle
```
1. Receive new header at height H
2. Create PDA for consensus state at H
3. Update client state (latest_height = H)
4. If window > 15 states:
   - Close oldest PDA
   - Reclaim rent (~0.002 SOL)
   - Update earliest_height
```

### Packet Lifecycle
```
1. Send:
   - Set bitmap bit
   - Create PacketCommitment PDA
   - Emit event

2. Finalize (ack/timeout):
   - Clear bitmap bit
   - Close PDA
   - Reclaim rent (~0.002 SOL)

3. Window slide:
   - Create overflow PDAs for active packets
   - Rotate bitmap
   - Update window_start
```

## Trade-offs

### Prunable Consensus States

**Pros:**
- 3,650x cost reduction vs permanent storage (730 SOL/year → 0.2 SOL one-time)
- Constant cost regardless of chain age
- Still maintains sufficient history for IBC

**Cons:**
- Cannot verify ancient packets (not needed in practice)
- Must track pruning window carefully
- Slightly more complex than append-only

### PDA-Based Packets

**Pros:**
- Rent reclamation on finalization
- Parallel processing
- No account size limits
- Idiomatic Solana pattern

**Cons:**
- Temporary rent lock (~0.002 SOL per packet)
- More accounts to manage

## Caller Experience

To simplify packet status queries, we provide view functions that abstract the storage complexity:

**View Functions (Free RPC Calls)**:
- `get_packet_status(sequence)` - Returns if packet is active
- `get_packet_commitment(sequence)` - Returns commitment hash if exists
- `get_window_info()` - Returns current window bounds and active count
- `get_consensus_state(client_id, height)` - Returns consensus state if in window

These abstract the storage layout from callers:
- Check bitmap for quick status
- Derive PDA only if commitment needed
- Handle window boundaries transparently
- No transaction fees (read-only)

**Benefits**:
- Single function call to check packet status
- No need to understand storage layout
- Consistent interface regardless of packet location
- Free to call via RPC

## Security Considerations

1. **Consensus State Availability**: Ensure pruning window > maximum packet lifetime
2. **Race Conditions**: Verify consensus state exists before pruning
3. **Emergency Recovery**: Can reconstruct from chain if needed
4. **Bitmap Integrity**: Only router can modify bitmap
5. **PDA Determinism**: Consistent seeds prevent duplicates
6. **Authority Checks**: Only authorized parties can close PDAs
7. **Pruning Timing**: Don't prune states with active packets
8. **Window Parameters**: Make configurable for different chains

## Cost Analysis

**Steady State Costs:**
```
Per Client:
- 100 consensus states × 0.002 SOL = 0.2 SOL

Per Channel:
- ~100 active packets × 0.002 SOL = 0.2 SOL
- RouterState account: ~0.002 SOL

10 Clients + 5 Channels:
- Total locked: ~3 SOL (constant, not annual)
- Transaction fees: ~0.00001 SOL per packet
```

**Annual Operating Cost (1000 packets/day, 1000 consensus states/day):**
```
Rent (rotating): ~0 (reclaimed)
Transaction fees:
  - Packets: 365,000 × 0.00001 = 3.65 SOL
  - Consensus: 365,000 × 0.00001 = 3.65 SOL
  - Total: ~7.3 SOL/year
```

This is 200x cheaper than permanent storage approaches.

## Configuration Parameters

```
CONSENSUS_STATE_WINDOW: 100  // Number of states to keep (~2.4 hours)
PACKET_WINDOW_SIZE: 10,000  // Bitmap size
RECV_COMMITMENT_INTERVAL: 100  // Packets per merkle commitment
PRUNING_DELAY: 3600  // Seconds before pruning eligible
```

## Implementation Notes

1. **Pruning Safety**: Always verify no active packets reference a consensus state before pruning
2. **Window Sizing**: Monitor typical packet lifetimes to optimize window sizes
3. **Batch Operations**: Consider batching PDA closures for efficiency
4. **Monitoring**: Track rent locked vs reclaimed metrics
5. **Graceful Degradation**: Handle missing consensus states gracefully

## Alternatives Considered

1. **Permanent Storage**: Too expensive (747.5 SOL/year)
2. **Dense Arrays**: No rent reclamation, account size limits
3. **Merkle Trees**: Complex, not Solana-native
4. **State Compression**: Not suitable for frequently accessed data
5. **No Bitmap**: Would require PDA derivation for every lookup

## Sum up

- Rolling window for consensus states (100 states)
- Bitmap + PDAs for packets

## References

- [IBC Packet Lifecycle](https://github.com/cosmos/ibc/tree/main/spec/core/ics-004-channel-and-packet-semantics)
- [Solana Account Model](https://solana.com/docs/core/accounts)
- [Solana PDAs](https://solana.com/docs/core/pda)
- [Solana Rent Economics](https://solana.com/docs/core/fees)
