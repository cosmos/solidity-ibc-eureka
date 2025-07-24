# ADR: Solana IBC Storage Architecture

**Status**: Proposed
**Date**: 2025-01-17

## Context

The Solana IBC implementation requires efficient storage mechanisms for both light client consensus states and packet lifecycle data. Unlike EVM chains where storage is managed within contract state, Solana's account model requires explicit decisions about data distribution across Program Derived Addresses (PDAs).

## Decision

We will implement a hybrid storage architecture that optimizes for different access patterns:

### Why Different Storage Strategies?

**Consensus States use PDAs because:**

- **Low write volume**: ~24 updates/day vs thousands of packets/day
- **Permanent storage**: Required indefinitely for IBC proof verification
- **Read-heavy pattern**: Many packets verify against same consensus state
- **Concurrency needs**: Multiple transactions can read different states in parallel
- **Cannot be pruned**: Future packets may reference old consensus states

**Packets use Bitmaps because:**

- **High write volume**: Potentially thousands per day on active channels
- **Transient lifecycle**: Only needed from send → acknowledge/timeout
- **Sequential processing**: Less benefit from parallel PDA access
- **Prunable**: Can be archived/removed after packet completes
- **Cost efficiency**: 365k packets/year would be 42x more expensive than consensus states

**Cost Example (1 year):**

```
Consensus States: 24/day × 365 = 8,760 PDAs × ~0.002 SOL = ~17.5 SOL
Packets (if PDA): 1000/day × 365 = 365,000 PDAs × ~0.002 SOL = ~730 SOL
Packets (bitmap + commitments): Single account rent + commitment storage = ~0.002 SOL + storage growth

```

### 1. Tendermint Light Client Storage (via PDAs)

**Rationale**: Light clients require long-term storage of consensus states for verification

- Each consensus state stored in separate PDA: `["consensus_state", client_key, height_bytes]`
- Client state in single PDA: `["client", chain_id]`
- No pruning of historical consensus states

**Benefits**:

- Parallel access for concurrent verifications
- No account size limits
- Direct verification by other programs
- Efficient selective loading

### 2. Packet Storage (Bitmap + Commitments)

**Rationale**: Packet data is transient and only needed during the packet lifecycle

- Use bitmap for tracking active packets within sliding window
- Store packet commitments (32 bytes each) for verification
- Window size: 10,000 packets (configurable)
- NO PDAs for packet data - all stored in router account
- Only track packets on source chain (destination chain tracks receives separately)

**Why Only One Bitmap?**
In IBC, the source chain only needs to know if a packet is "active" (sent but not finalized):

- **Received status**: Tracked by destination chain, not needed on source
- **Acknowledged status**: Once acknowledged, packet is done - just clear the bit
- **Timeout status**: Once timed out, packet is done - just clear the bit

This dramatically simplifies storage from 3 bits per packet to just 1 bit.

**Storage Strategy**:
Since we store individual packet commitments (32 bytes per packet), we can verify packets immediately without needing a merkle root during operations.

**Merkle Root for Archival**:

- During operation: Store bitmap + individual commitments
- At window slide: Compute merkle root from stored commitments
- Archive merkle root for efficient batch verification of historical data
- Individual commitments provide real-time verification capability

This approach provides:

- Immediate packet verification via stored commitments
- Efficient historical verification via merkle roots
- No expensive merkle tree updates during packet sends

**Implementation**:

```rust
pub struct RouterState {
    // Primary window
    pub sent_packets: BitVec,         // 1 = active (sent, not finalized), 0 = not sent or finalized
    pub window_start: u64,            // First sequence in window
    pub window_size: u64,             // Size of sliding window

    // Packet commitments for verification (32 bytes per packet)
    pub packet_commitments: BTreeMap<u64, [u8; 32]>, // sequence -> hash(packet)


    // Active packets outside current window (sparse storage)
    pub active_packets: Vec<u64>, // Just sequence numbers of active packets
    pub active_packet_commitments: BTreeMap<u64, [u8; 32]>,
}

// Packet commitment storage - separate PDAs for each commitment
// PDA: ["packet_commitment", channel_id.as_bytes(), sequence.to_le_bytes()]
pub struct PacketCommitment {
   pub commitment: [u8; 32],         // hash(packet)
   pub timestamp: i64,               // For TTL enforcement
}
```

**State Transitions**:

- Send packet: Set bit to 1, create PacketCommitment PDA with hash
- Receive acknowledgment: Set bit to 0, close PacketCommitment PDA to reclaim rent
- Timeout packet: Verify commitment PDA exists and matches, set bit to 0, close PDA
- Check if sent: Read bit (1 = yes and active, 0 = no or already finalized)
- Window slide: Extract active packets to active_packets vector, close PDAs for finalized packets

**Window Management**:

- Sequence number maps to bitmap index: `index = sequence - window_start`
- Window slides when ~80% full or when early portion has no active packets
- Active packets are extracted to `active_packets` set before sliding
- Only stores sequence numbers of in-flight packets outside window

**Storage Bounds and Limits (example)**:

- Window size: 10,000 packets
- Maximum active packets outside window: 1,000
- Packet TTL: 24 hours
- Total commitment limit: 11,000 (window + active)

Enforcement:

- Reject sends when active packets exceed limit
- Expire packets older than TTL
- Prevent commitment map overflow

This ensures:

- Maximum 11,000 packets tracked at any time
- Packets expire after 24 hours
- Prevents DoS via unbounded storage growth

**Storage Size Calculation**:

```
Base RouterState: ~200 bytes
Bitmap (10,000 bits): 1,250 bytes
Commitments (11,000 × 32): 352,000 bytes
Active packets set (1,000 × 8): 8,000 bytes
Total: ~361KB (well within 10MB limit)

```

### 3. Off-chain Archival System

**Rationale**: Historical packet data needed for debugging/compliance but not for protocol operation (unless packet is still active)

**Cryptographic Archive Proof**:

```rust
pub struct ArchiveBundle {
    pub window_start: u64,
    pub window_end: u64,
    pub packets: Vec<PacketData>,
    pub merkle_root: [u8; 32],
}

pub struct ArchiveCommitment {
    pub bundle_hash: [u8; 32],      // Hash of entire archive bundle
    pub merkle_root: [u8; 32],       // Merkle root of packets in bundle
    pub archive_uri: String,         // URI for retrieval (IPFS CID, Arweave ID, HTTPS URL, etc.)
    pub archived_at: u64,            // Block height when archived
}

```

**Archive Process**:

1. Before sliding window, collect all packet data from events
2. Compute merkle tree from stored commitments in packet_commitments
3. Create archive bundle with packets and computed merkle root
4. Upload bundle to off-chain storage (IPFS/Arweave/S3/custom service - TBD)
5. Store archive metadata in off-chain index (not on-chain)

**Verification Process**:
Archives are verified through the off-chain indexer which maintains:

- Archive URIs and metadata
- Merkle roots for each archived window
- Bundle hashes for integrity verification

To verify historical packets:

- Query indexer for archive metadata
- Retrieve bundle from storage service
- Verify packet membership using merkle proofs
- Note: This relies on indexer integrity, not on-chain guarantees

**Non-Membership Proof Verification**:
For timeout proofs, we need to verify the packet was NOT received on the destination chain. This requires:

1. **Destination Chain State**: The destination chain must store packet receipts in a provable structure (merkle tree, IAVL, etc.)
2. **Proof Components**:
    - Merkle proof showing absence at the expected path
    - Consensus state of destination chain at proven height
    - Light client verification of the proof
3. **Solana's Limitation**: Solana doesn't have native merkle trees for arbitrary data.
4. **Our Approach for Solana as Destination**:

    **Receipt Storage Structure**:

    ```rust
    pub struct ReceivedPacketState {
        pub received_packets: BitVec,              // 1 = received, 0 = not received
        pub receipt_commitments: BTreeMap<u64, [u8; 32]>, // sequence -> receipt hash
        pub last_commitment_at: u64,               // Last sequence where we updated commitment
        pub chunk_commitments: BTreeMap<u64, [u8; 32]>,   // chunk_start -> merkle_root
    }

    ```

    **Bitmap + Merkle Commitment Approach**:
    Instead of maintaining a full merkle tree, we use periodic commitments of bitmap chunks:

    1. **Bitmap Storage**: Track received packets with 1 bit per packet
    2. **Periodic Commitments**: Every 100 packets, compute merkle root of that chunk
    3. **Chunk-based Proofs**: Non-membership proofs use bitmap segments + merkle commitment

    **Non-Membership Proof Generation**:

    - Divide bitmap into chunks of 100 packets each
    - For each chunk, periodically compute merkle root of bitmap segment
    - Store chunk commitment: `chunk_commitments[chunk_start] = merkle_root`
    - Non-membership = proving bit is 0 in the relevant chunk

    **Example for sequence 1234**:

    - Chunk: 1200-1299 (chunk_start = 1200)
    - Bit position: 1234 - 1200 = 34
    - Proof: Show bit 34 is 0 in chunk 1200's committed bitmap

    **Computational Efficiency**:

    - **Amortized cost**: ~325 CU per receive (32,500 CU ÷ 100 packets)
    - **Batch updates**: Merkle root computed once per 100 packets
    - **Immediate verification**: Individual bits can be checked without merkle operations

    **Storage Efficiency**:

    - **Bitmap**: 1 bit per packet (10k packets = 1.25KB)
    - **Chunk commitments**: 32 bytes per 100 packets (10k packets = 3.2KB)
    - **Total**: ~4.5KB for 10k packets (vs 320KB for full merkle tree)

## Storage Lifecycle

### Light Client Updates

```
1. Receive new header at height H
2. Create PDA for consensus state at H
3. Update latest_height in client state
4. Historical states remain for future proofs

```

### Packet Lifecycle

```
1. Send:
   a. Set bit to 1 in sent_packets bitmap
   b. Store packet commitment hash in packet_commitments
   c. Emit event with full packet data
2. Receive: Destination chain handles this (source chain uninvolved)
3. Acknowledge:
   a. Clear bit to 0 (packet finalized)
   b. Remove commitment from packet_commitments
4. Timeout:
   a. Relayer provides original packet data
   b. Verify packet is active (bit = 1)
   c. Verify packet hash matches stored commitment
   d. Verify timeout proof (non-membership + height/time exceeded)
   e. Clear bit to 0, remove commitment
5. Window Sliding:
   a. Check if window is >80% full or early portion has no active packets
   b. Extract active packet commitments for archival
   c. Upload archive bundle to off-chain storage service
   d. Store archive commitment on-chain with merkle root
   e. Move active packets to active_packets set
   f. Update window_start to new position
   g. Clear bitmap and old commitments

```

**Window Sliding Process**:

1. Extract active packets from sliding portion to active_packets set
2. Clean up commitments for finalized packets
3. Update window_start position
4. Clear bitmap for reuse

**Acknowledgment Handling**:

- Check if packet in current window or active set
- Clear bitmap bit if in window
- Remove from active set if outside window
- Clean up commitment entry

**Timeout Handling**:

- Verify packet is active (check bitmap or active set)
- Verify provided packet data matches stored commitment
- Verify timeout proofs (non-membership + height exceeded)
- Clear packet state and commitment

## Trade-offs

### Consensus State Storage

- **Pros**: Unlimited history, parallel access, composability
- **Cons**: Linear growth in accounts, rent costs accumulate

### Packet Bitmap + Commitments Approach

- **Pros**:
    - Reasonable storage (1 bit + 32 bytes commitment per packet)
    - No PDA rent costs
    - Packet verification without full data
    - Prevents timeout fraud
    - Graceful handling of in-flight packets via active set
    - Source chain only tracks what it needs
- **Cons**:
    - 32 bytes per packet storage cost
    - Requires events for full packet data (but mitigated by indexer)
    - Must check both window and active set for lookups
    - Commitment cleanup needed on finalization

## Security Considerations

1. **Window Size**: Must be large enough to handle network delays but small enough to fit in account (10 mb should be more than enough)
2. **Archive Integrity**: Merkle tree and bundle hash provide cryptographic proof of archive contents
3. **Bitmap Manipulation**: Only router program can modify bitmap
4. **Event Reliability**: Events must be reliably indexed for packet data retrieval
5. **Replay Protection**: Bitmap must be checked before processing any packet
6. **Active Packet Cleanup**: Active packets set cleaned when packets finalize
7. **Double Spend**: Must check both current window and active packets set to prevent replay attacks
8. **Timeout Data**: Relayers provide packet data, verified against stored commitments
9. **Commitment Verification**: Timeout handler must verify packet data matches the commitment
10. **Storage Bounds**: Enforce MAX_ACTIVE_PACKETS (1000) and PACKET_TTL (24h) to prevent DoS
11. **Account Size Limits**: Monitor total storage to stay within Solana's 10MB account limit

## Concurrency and Race Conditions

**Challenge**: Solana processes transactions in parallel, creating potential race conditions when multiple packets are sent/acknowledged simultaneously.

**Solution: Account-Based Locking**

```rust
// Separate account for write locking
pub struct SequenceLock {
    pub next_sequence: u64,
    pub authority: Pubkey,
}

```

**Approach 1: Sequential Sequence Assignment**

- Use a separate `SequenceLock` account (per channel) that must be writable for send operations
- Solana runtime automatically serializes access to this account
- Each send transaction:
    1. Reads current `next_sequence` from lock account
    2. Assigns this sequence to the new packet
    3. Increments `next_sequence` for the next packet
    4. Updates bitmap at index `sequence - window_start`

**Throughput Analysis**:

- Solana block time: ~400ms
- Maximum theoretical: ~2.5 packets/second per channel
- Real-world throughput: ~1-2 packets/second due to:
    - Network latency
    - Transaction confirmation time
    - MEV/priority fee competition
- **For most IBC use cases, this is sufficient** (cross-chain transfers are inherently slower)
- Multiple channels can operate in parallel without interference

**Approach 2: Pre-allocated Sequence Ranges**

- Allocate sequence ranges to different relayers/users
- Each range owner can send packets in parallel within their range
- Requires coordination but allows parallel sends

**Approach 3: Optimistic Concurrency Control**

- Allow parallel bitmap updates with conflict detection
- Use Solana's account versioning to detect conflicts
- Retry on conflicts (higher complexity)

**Recommended: Approach 1** for simplicity and correctness, with option to upgrade to Approach 2 if throughput becomes a bottleneck.

**Alternative: Multiple Router Accounts**:

```rust
// Shard router state across multiple accounts for parallelism
pub struct RouterShard {
    pub shard_id: u8,                   // 0-3 for 4 shards
    pub sequence_range: Range<u64>,     // e.g., 0-2500, 2500-5000
    pub sent_packets: BitVec,
    pub packet_commitments: BTreeMap<u64, [u8; 32]>,
}

```

This enables 4x parallel throughput (~10 packets/second) but adds complexity in cross-shard coordination.

## Caller Experience

To simplify packet status queries across window and overflow storage, we'll implement view functions that abstract the dual-location lookup:

**View Functions (Free RPC Calls)**:

- `get_packet_status(sequence)` - Returns if packet is active
- `get_window_info()` - Returns current window bounds and active count

These abstract the dual-location lookup (window bitmap + active set) from callers.

**Benefits for Callers**:

- Single function call to check packet status
- No need to understand storage layout
- Free to call (no transaction fees)
- Consistent interface regardless of packet location
- Cryptographic verification of archived data

**Usage Example**:

```jsx
// Relayer checking packet status
const status = await program.methods
    .getPacketStatus(sequence)
    .view(); // Free RPC call

if (status.active) {
    // Packet is still in flight, can be relayed/timed out
}

```

## Alternatives Considered

1. **All PDAs** (Current): Simple but unbounded growth
2. **Single Account Maps**: Hit size limits, poor concurrency
3. **Merkle Trees**: Complex, requires off-chain data availability as non-membership is not included in Solana
4. **State Compression**: Not suitable for frequently accessed data
5. **Merkle Root Only (No Individual Commitments)**:
    - Store only merkle root, compute at archive time
    - Would save 32 bytes per packet
    - Cannot verify packet data during timeout before archival
    - Allows relayers to fabricate packet data

    Accept the 32 bytes per packet cost for security


## References

- [Solana Account Model](https://solana.com/docs/core/accounts)
- [IBC Packet Lifecycle](https://github.com/cosmos/ibc/tree/main/spec/core/ics-004-channel-and-packet-semantics)
