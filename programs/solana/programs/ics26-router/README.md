# ICS26 Router

Core IBC packet routing for Solana. Handles packet commitments, sequence management, and routing between IBC apps and light clients.

## Range-Based Commitments

Standard IBC uses one account per packet: `PDA = [seed, client_id, sequence]`

This breaks with RPC lag. When you query the sequence, you get stale data (10-20 slots old). By the time your transaction lands, someone else used that sequence. Transaction fails, you retry, waste compute and fees. Under load this limits you to ~0.1-1 packet/second.

We fix this by batching 100 commitments into one account:

```rust
// Client queries sequence (stale): ~247
range_index = sequence / 100  // 2
PDA = [seed, client_id, range_index]  // Just need the right range, not exact sequence

// On-chain (atomic):
actual_sequence = counter.read()  // 251 (the real value)
slot = actual_sequence % 100  // 51
commitment_range.commitments[slot] = hash(packet)
counter.increment()
```

Client can be off by ±99 sequences and still hit the correct range account. On-chain atomic counter ensures no collisions.

## How it works

Each `CommitmentRange` account has:
- 16-byte bitmap tracking which of 100 slots are used
- 3200 bytes for 100 commitments (32 bytes each)

We use direct memory access to avoid stack overflow (3KB struct is too big for Solana's 4KB stack):

```rust
// Don't load entire struct, just read/write specific offsets
let bitmap_offset = 8 + 1;  // discriminator + version
let commitment_offset = bitmap_offset + 16 + (slot * 32);

// Read/write directly
data[commitment_offset..commitment_offset+32] = commitment;
data[bitmap_offset + slot/8] |= 1 << (slot % 8);
```

When all 100 slots are cleared, we close the account and return rent.

## Example

Two users send packets with same stale RPC data (seq=247):

```
Both compute: range_index = 247 / 100 = 2
Both reference: CommitmentRange #2

Solana serializes writes to same account:
  TX A: reads seq=247, writes slot 47, increments to 248
  TX B: reads seq=248, writes slot 48, increments to 249

Both succeed.
```

## Testing

```bash
just test-solana           # Unit tests
just test-e2e-solana       # Integration tests
```
