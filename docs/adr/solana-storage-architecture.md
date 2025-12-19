# ADR: Solana IBC Storage Architecture

**Status**: Implemented
**Date**: 2025-06-17
**Updated**: 2025-12-19

## Context

The Solana IBC implementation requires efficient storage mechanisms for both light client consensus states and packet lifecycle data. Unlike EVM chains where storage is managed within contract state, Solana's account model requires explicit decisions about data distribution across Program Derived Addresses (PDAs). Solana programs cannot use standard Rust collections like `BTreeMap` or `BTreeSet`, and the idiomatic approach is to use PDAs as a key-value store.

## Decision

We implement a PDA-based storage architecture with chunked storage for large data that exceeds Solana's transaction size limits.

### Key Design Principles

1. **PDA-Based Storage**: All state stored in Program Derived Addresses
2. **Chunked Uploads**: Large data (headers, proofs, payloads) split into ~900-byte chunks
3. **Rent Reclamation**: Temporary accounts (chunks, commitments) closed after use
4. **Access Control**: Centralized access-manager program for role-based permissions

## Storage Architecture

### 1. ICS07-Tendermint Light Client Storage

**Client State PDA:**
```
Seeds: [b"client", chain_id.as_bytes()]
```
Stores:
- `chain_id`: String (max 64 bytes)
- `trust_level_numerator`, `trust_level_denominator`: u64
- `trusting_period`, `unbonding_period`, `max_clock_drift`: u64
- `frozen_height`, `latest_height`: IbcHeight { revision_number, revision_height }

**Consensus State PDAs:**
```
Seeds: [b"consensus_state", client_state_pubkey.as_ref(), height.to_le_bytes()]
```
Stores:
- `height`: u64
- `consensus_state`: ConsensusState
  - `timestamp`: u64 (nanoseconds since Unix epoch)
  - `root`: [u8; 32] (commitment root)
  - `next_validators_hash`: [u8; 32]

**App State PDA:**
```
Seeds: [b"app_state"]
```
Stores:
- `access_manager`: Pubkey
- `_reserved`: [u8; 256]

### 2. ICS26-Router Storage

**Router State PDA:**
```
Seeds: [b"router_state"]
```
Stores:
- `version`: AccountVersion
- `access_manager`: Pubkey
- `_reserved`: [u8; 256]

**Client Registry PDA:**
```
Seeds: [b"client", client_id.as_bytes()]
```
Stores:
- `version`: AccountVersion
- `client_id`: String (max 64 bytes)
- `client_program_id`: Pubkey (light client program)
- `counterparty_info`: CounterpartyInfo { client_id, merkle_prefix }
- `active`: bool
- `_reserved`: [u8; 256]

**Client Sequence PDA:**
```
Seeds: [b"client_sequence", client_id.as_bytes()]
```
Stores:
- `version`: AccountVersion
- `next_sequence_send`: u64 (starts at 1 per IBC spec)
- `_reserved`: [u8; 256]

**IBC App Registry PDA:**
```
Seeds: [b"ibc_app", port_id.as_bytes()]
```
Stores:
- `version`: AccountVersion
- `port_id`: String (max 128 bytes)
- `app_program_id`: Pubkey
- `authority`: Pubkey
- `_reserved`: [u8; 256]

### 3. Packet Commitment Storage

All packet commitments are 32-byte SHA256 hashes stored in Commitment PDAs.

**Packet Commitment (for sent packets):**
```
Seeds: [b"packet_commitment", source_client.as_bytes(), sequence.to_le_bytes()]
```

**Packet Receipt (for received packets):**
```
Seeds: [b"packet_receipt", dest_client.as_bytes(), sequence.to_le_bytes()]
```

**Packet Acknowledgement:**
```
Seeds: [b"packet_ack", dest_client.as_bytes(), sequence.to_le_bytes()]
```

### 4. Chunked Storage Pattern

Large data exceeding transaction limits is split into 900-byte chunks uploaded in separate transactions, then assembled.

**Header Chunk (for client updates):**
```
Seeds: [b"header_chunk", submitter.as_ref(), chain_id.as_bytes(), target_height.to_le_bytes(), chunk_index]
```
Stores:
- `submitter`: Pubkey
- `chunk_data`: Vec<u8> (max 900 bytes)

**Payload Chunk (for large packet payloads):**
```
Seeds: [b"payload_chunk", payer.as_ref(), client_id.as_bytes(), sequence.to_le_bytes(), payload_index, chunk_index]
```
Stores:
- `client_id`: String
- `sequence`: u64
- `payload_index`: u8 (for multi-payload packets)
- `chunk_index`: u8
- `chunk_data`: Vec<u8> (max 900 bytes)

**Proof Chunk (for membership/verification proofs):**
```
Seeds: [b"proof_chunk", payer.as_ref(), client_id.as_bytes(), sequence.to_le_bytes(), chunk_index]
```
Stores:
- `client_id`: String
- `sequence`: u64
- `chunk_index`: u8
- `chunk_data`: Vec<u8> (max 900 bytes)

**Misbehaviour Chunk (for misbehaviour reports):**
```
Seeds: [b"misbehaviour_chunk", submitter.as_ref(), client_id.as_bytes(), chunk_index]
```
Stores:
- `chunk_data`: Vec<u8> (max 900 bytes)

**Signature Verification (cached verification results):**
```
Seeds: [b"sig_verify", signature_hash]
```
Stores:
- `submitter`: Pubkey
- `is_valid`: bool

### 5. Chunked Upload Workflow

```
1. Upload Phase:
   - Relayer uploads chunks in separate transactions
   - Each chunk stored in deterministic PDA
   - Per-submitter PDAs enable cleanup of failed uploads

2. Assembly Phase:
   - Single transaction reads all chunk accounts
   - Assembles full data (header/proof/payload)
   - Triggers verification/processing

3. Cleanup Phase:
   - Chunk accounts closed after successful assembly
   - Rent (~0.01 SOL per chunk) reclaimed to payer
```

### 6. Access Control Integration

The `access-manager` program provides centralized role-based access control.

**Access Manager PDA:**
```
Seeds: [b"access_manager"]
```
Stores:
- `roles`: Vec<RoleData> (max 16 entries)
  - Each RoleData: { role_id: u64, members: Vec<Pubkey> }

**Upgrade Authority PDA:**
```
Seeds: [b"upgrade_authority", target_program.as_ref()]
```

Programs reference `access_manager` pubkey in their state to validate permissions via CPI.

## Storage Lifecycle

### Packet Lifecycle
```
1. Send:
   - Increment sequence in ClientSequence
   - Create PacketCommitment PDA
   - Emit event

2. Receive:
   - Create PacketReceipt PDA (prevents replay)
   - Execute app callback
   - Create PacketAck PDA

3. Acknowledge:
   - Verify ack proof against commitment
   - Close PacketCommitment PDA
   - Reclaim rent

4. Timeout:
   - Verify non-receipt on destination
   - Close PacketCommitment PDA
   - Reclaim rent
```

### Client Update Lifecycle
```
1. Upload header chunks (multiple transactions)
2. Assemble and verify header (single transaction)
3. Create/update ConsensusState PDA
4. Close header chunk accounts
5. Reclaim rent
```

## Cost Analysis

**Account Rent Costs (approximate):**
```
Per account rent: ~0.01 SOL (refundable when account closed)
```

**Real-World Client Update Costs (Measured on Mainnet):**

| Chain        | Validators | 2/3 Threshold | Prep Txs    | Total CUs | Total Cost (SOL) | USD Cost      | Latency |
| ------------ | ---------- | ------------- | ----------- | --------- | ---------------- | ------------- | ------- |
| **Noble**    | 20         | ~14 sigs      | 19 parallel | ~548k     | 0.000166         | ~$0.025-0.033 | ~18s    |
| **Celestia** | 100        | ~67 sigs      | 83 parallel | ~2.16M    | 0.000721         | ~$0.11-0.14   | ~22s    |

*Costs at $150-200/SOL. Latency depends on RPC throttling.*

**Cost Breakdown:**
- Transaction base fees: 5,000 lamports/tx
- Priority fees: variable (market-driven)
- Chunk rent: ~0.01 SOL per chunk (refunded after assembly)
- Consensus state rent: ~0.01 SOL (permanent)

**Per-Packet Cost:**
```
- Commitment creation: ~0.01 SOL (refunded on ack/timeout)
- Transaction fees: ~0.000005 SOL
- Net cost after reclaim: ~0.000005 SOL
```

**Key Insights:**
- Cost scales roughly linearly with validator count (~5x validators = ~4x cost)
- Chunk and commitment rent is fully refundable
- Relayers must call cleanup instructions to reclaim rent

## Security Considerations

1. **PDA Determinism**: Consistent seeds prevent duplicate accounts
2. **Authority Checks**: Only authorized parties can modify state
3. **Chunk Ownership**: Per-submitter PDAs prevent interference
4. **Access Control**: Role-based permissions via access-manager
5. **Commitment Integrity**: Only router can create/close commitment PDAs

## Configuration Constants

```rust
pub const CHUNK_DATA_SIZE: usize = 900;
pub const MAX_CLIENT_ID_LENGTH: usize = 64;
pub const MAX_PORT_ID_LENGTH: usize = 128;
```

## Future Considerations

The following features are not currently implemented but may be added:

1. **Consensus State Pruning**: Rolling window to limit storage growth
2. **Archival System**: Off-chain storage for historical packet data
3. **Batch Operations**: Batching PDA closures for efficiency

## References

- [IBC Packet Lifecycle](https://github.com/cosmos/ibc/tree/main/spec/core/ics-004-channel-and-packet-semantics)
- [Solana Account Model](https://solana.com/docs/core/accounts)
- [Solana PDAs](https://solana.com/docs/core/pda)
- [Solana Rent Economics](https://solana.com/docs/core/fees)
