# ICS07 Tendermint Light Client for Solana

This is a Solana implementation of the ICS07 Tendermint light client, enabling IBC (Inter-Blockchain Communication) between Solana and Tendermint-based chains.

## Overview

The Tendermint light client verifies consensus proofs from Tendermint chains on Solana. This implementation supports multiple independent light client instances, each tracking a different Tendermint-based chain. After initialization, each client instance is identified by its chain ID and can be used to verify proofs from its corresponding chain.

Since Tendermint headers are always several KB in size and exceed Solana's transaction size limitations (~1232 bytes), all headers must be uploaded in chunks across multiple transactions before being assembled and verified. Each chunk can contain up to 900 bytes of header data.

## Architecture

### Chunked Upload System

The client implements a mandatory chunked upload mechanism for all header updates:

1. **Headers are split into chunks**: Large headers are divided into manageable pieces that fit within Solana transactions
2. **Metadata creation is separate**: The `create_metadata` instruction initializes the upload metadata once per upload
3. **Full parallel upload support**: All chunks can be uploaded in parallel after metadata creation for maximum throughput
4. **Each relayer has isolated storage**: Chunks are stored in PDAs derived from the relayer's address, preventing interference between relayers
5. **Relayers pay rent**: The submitting relayer pays for all storage (metadata and chunk accounts)
6. **Assembly triggers verification**: Once all chunks are uploaded, the relayer calls `assemble_and_update_client` to reconstruct and verify the header

### Key Design Principles

- **Relayer Isolation**: Each relayer's uploads are completely isolated via PDAs keyed by submitter address
- **Rent Economics**: Relayers are incentivized to clean up failed uploads as they pay the rent
- **Atomic Updates**: Header assembly and client updates happen atomically - either all succeed or all fail
- **Automatic Cleanup**: Successful assembly automatically closes temporary accounts and returns rent

## Program Instructions

### Core IBC Instructions

#### `initialize`
Initializes a new Tendermint light client instance for a specific chain. Multiple clients can be initialized to track different Tendermint-based chains simultaneously.

**Parameters:**
- `chain_id`: The unique chain identifier (e.g., "cosmoshub-4", "osmosis-1", "noble-1")
- `latest_height`: Initial trusted height
- `client_state`: Initial client configuration (trust level, periods, etc.)
- `consensus_state`: Initial trusted consensus state

**Accounts:**
- `client_state` (init): PDA storing client configuration, derived from chain_id
- `consensus_state_store` (init): PDA storing consensus state at height
- `payer` (signer, mut): Account paying for initialization
- `system_program`: System program

**Multi-Chain Support**: Each chain_id creates a separate client instance with its own state. This allows Solana to maintain IBC connections with multiple Tendermint chains concurrently.

### Chunked Upload Instructions

Since Tendermint headers always exceed Solana's transaction size limits, all header updates must use the chunked upload system described below.

#### `create_metadata`
Creates metadata for a header upload. This instruction must be called once before uploading chunks.

**Parameters:**
- `chain_id`: Target chain identifier
- `target_height`: Height being updated to
- `total_chunks`: Total number of chunks expected
- `header_commitment`: Keccak hash of the complete header

**Accounts:**
- `metadata` (init): PDA for header metadata
- `client_state`: Validates chain exists
- `submitter` (signer, mut): Relayer creating metadata and paying rent
- `system_program`: System program

**Notes:**
- Must be called exactly once per upload attempt
- Creates a new metadata account for tracking the upload

#### `upload_header_chunk`
Uploads a single chunk of a large header. Requires metadata to be created first via `create_metadata`.

**Parameters:**
- `params`: UploadChunkParams containing:
  - `chain_id`: Target chain
  - `target_height`: Height being updated to
  - `chunk_index`: Position of this chunk (0-indexed)
  - `chunk_data`: The actual chunk bytes (max 900 bytes)
  - `chunk_hash`: Keccak hash of the chunk data for integrity verification

**Accounts:**
- `chunk` (init_if_needed): PDA for this specific chunk
- `metadata` (mut): PDA for header metadata (must already exist)
- `client_state`: Validates chain exists
- `submitter` (signer, mut): Relayer uploading and paying rent
- `system_program`: System program

**Storage Cost**: Each chunk account costs rent, paid by submitter

**Validation**: The instruction validates that the chunk's chain_id and target_height match the metadata, and that the chunk_index is within the expected range (< total_chunks from metadata).

**Parallel Upload**: After metadata is created, all chunks can be uploaded in parallel transactions for faster throughput. Each chunk upload is independent.

#### `assemble_and_update_client`
Assembles uploaded chunks into a complete header and updates the client.

**Accounts:**
- `client_state` (mut): Client being updated
- `metadata` (mut, close): Header metadata (closed after success)
- `trusted_consensus_state`: Consensus at trusted height
- `new_consensus_state_store`: New consensus state account
- `submitter` (mut): Original submitter (receives rent back)
- `payer` (signer, mut): Pays for new consensus state
- `system_program`: System program
- Remaining accounts: Chunk accounts in order (all closed after success)

**Process:**
1. Validates all chunks are present and match commitment
2. Reconstructs complete header from chunks
3. Verifies header against trusted state
4. Updates client state
5. Closes all temporary accounts, returning rent to submitter

#### `cleanup_incomplete_upload`
Allows relayers to reclaim rent from failed or abandoned uploads.

**Parameters:**
- `chain_id`: Chain identifier
- `cleanup_height`: Height of upload to clean
- `submitter`: Original submitter address

**Accounts:**
- `client_state`: Validates chain exists
- `metadata` (mut, close): Metadata to close
- `submitter_account` (signer, mut): Must be original submitter
- Remaining accounts: Chunk accounts to close

**Security**: Only the original submitter can clean up their own uploads

### Verification Instructions

#### `verify_membership`
Verifies a key-value pair exists in the counterparty chain's state.

**Parameters:**
- `msg`: MembershipMsg with proof details

**Accounts:**
- `client_state`: Client configuration
- `consensus_state_at_height`: Consensus state at proof height

#### `verify_non_membership`
Verifies a key does not exist in the counterparty chain's state.

**Parameters:**
- `msg`: MembershipMsg with proof details

**Accounts:**
- `client_state`: Client configuration
- `consensus_state_at_height`: Consensus state at proof height

### Misbehaviour Handling

#### `submit_misbehaviour`
Submits evidence of misbehaviour to freeze the client.

**Parameters:**
- `msg`: MisbehaviourMsg with conflicting headers

**Accounts:**
- `client_state` (mut): Client to potentially freeze
- `trusted_consensus_state_1`: First trusted state
- `trusted_consensus_state_2`: Second trusted state

## PDA Derivations

All storage uses Program Derived Addresses (PDAs) for deterministic addressing. The chain_id is a key component in most PDAs, ensuring complete isolation between different chain clients:

```
client_state: [b"client", chain_id]
consensus_state: [b"consensus_state", client_state, height_bytes]
header_chunk: [b"header_chunk", submitter, chain_id, height_bytes, chunk_index]
header_metadata: [b"header_metadata", submitter, chain_id, height_bytes]
```

This PDA structure ensures that:
- Each chain has its own isolated client state
- Consensus states are chain-specific
- Upload operations cannot interfere across different chains
- Multiple chains can be tracked simultaneously without conflicts

## Upload Flow Example

```
1. Relayer receives 3.6KB Tendermint header
2. Splits into 4 chunks of 900 bytes each
3. Creates metadata with create_metadata instruction
4. Uploads all chunks (can be done in parallel):
   - Chunks 0-3: All uploaded independently in parallel
5. Calls assemble_and_update_client:
   - Header reconstructed and verified
   - Client state updated
   - All 5 temporary accounts closed (metadata + 4 chunks)
   - Rent (~0.05 SOL) returned to relayer
```

### Parallel Upload Optimization

For maximum throughput with large headers:
1. Call `create_metadata` to initialize metadata
2. Upload all chunks in parallel transactions
3. Wait for all confirmations
4. Call `assemble_and_update_client`

This parallel approach can reduce upload time from `n * block_time` to `2 * block_time` for n chunks.

## Rent and Economics

- **Temporary Storage**: Chunks and metadata are temporary, existing only during upload
- **Rent Responsibility**: Uploading relayer pays all rent (~0.01 SOL per account)
- **Automatic Refund**: Successful assembly returns all rent to the submitter
- **Cleanup Incentive**: Failed uploads can be cleaned up by submitter to reclaim rent
- **No Cross-Relayer Interference**: Each relayer's uploads are isolated

## Security Considerations

1. **Permissioned Cleanup**: Only the original submitter can clean up their uploads
2. **Commitment Verification**: Header commitment prevents chunk tampering
3. **Height Validation**: Cannot upload chunks for already-processed heights
4. **Client Freezing**: Misbehaviour detection can freeze compromised clients
5. **Trusted Height Validation**: Updates must reference valid trusted heights

## Testing

The implementation includes comprehensive tests for:
- Happy path updates with real Tendermint fixtures
- Chunked upload and assembly
- Error conditions (missing chunks, wrong order, corruption)
- Misbehaviour detection
- Rent reclamation
- Multi-relayer scenarios

Run tests:
```bash
cargo test --package ics07-tendermint
```

## Gas/Compute Costs

Approximate compute units per operation:
- `initialize`: ~50k CU
- `upload_header_chunk`: ~30k CU per chunk
- `assemble_and_update_client`: ~200k CU (includes verification)
- `verify_membership`: ~100k CU
- `verify_non_membership`: ~100k CU
- `submit_misbehaviour`: ~150k CU
- `cleanup_incomplete_upload`: ~20k CU per chunk

## Integration Guide

### For Relayers

1. **Choose the target chain**: Determine which Tendermint chain you're relaying for (each chain_id has its own client)
2. Monitor Tendermint chain for new headers
3. Split header into 900-byte chunks
4. Create metadata via `create_metadata` (specify the correct chain_id)
5. Upload all chunks in parallel for optimal performance
6. Call `assemble_and_update_client` once all chunks are confirmed
7. Handle failures:
   - Retry failed chunks
   - Call `cleanup_incomplete_upload` if abandoning (will need to start fresh with new metadata)

**Multi-Chain Relaying**: Relayers can operate across multiple chains simultaneously. Each chain's uploads are isolated by chain_id in the PDA derivation.

**Performance Tip**: With the separated metadata creation, all chunks can now be uploaded in parallel. A 9KB header (10 chunks of 900 bytes) can be uploaded in ~2 block times instead of ~10.

### For IBC Applications

1. **Specify the chain**: Reference the specific client via chain_id PDA
2. Call `verify_membership` for packet proofs from that chain
3. Check client not frozen before relying on proofs
4. Monitor for client updates
5. **Multi-Chain Applications**: Can interact with multiple chains by referencing different chain_id PDAs

