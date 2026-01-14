# ADR: Interchain Fungible Token (IFT) Architecture for Solana

**Status**: Implemented
**Date**: 2025-01-04
**Last Updated**: 2026-01-08

IFT for Solana uses a **burn-and-mint pattern** with ICS27-GMP for cross-chain messaging—no escrow accounts, minimal SPL token operations.

## Architecture

### Program Layering

```
┌─────────────────────────────────────────────────────┐
│                      IFT Program                     │
│  • Burn/mint SPL tokens                              │
│  • Pending transfer tracking                         │
│  • Refund on failure/timeout                         │
└──────────────────────┬──────────────────────────────┘
                       │ CPI (send_call)
┌──────────────────────▼──────────────────────────────┐
│                     GMP Program                      │
│  • Cross-chain payload encoding                      │
│  • Empty salt enforcement                            │
│  • GMP account PDA derivation                        │
└──────────────────────┬──────────────────────────────┘
                       │ CPI (send_packet)
┌──────────────────────▼──────────────────────────────┐
│                   Router Program                     │
│  • Light client verification                         │
│  • Packet commitment/acknowledgment                  │
│  • Sequence management                               │
└─────────────────────────────────────────────────────┘
```

**Note on CPI Validation**: Due to Solana's instruction sysvar limitation (which only exposes the top-level program, not immediate CPI caller), IFT must be registered as an "upstream caller" for GMP's port in the Router. See [GMP ADR](./solana-ics27-gmp-architecture.md#cpi-caller-validation-limitation) for details.

## SPL Token Operations

### Operations Used

| Operation | Instruction | Usage |
|-----------|-------------|-------|
| **Burn** | `ift_transfer.rs` | Burn tokens when initiating cross-chain transfer |
| **Mint** | `ift_mint.rs` | Mint tokens to receiver on incoming transfer |
| **Mint** | `on_ack_packet.rs` | Refund on failed transfer (mint back to sender) |
| **Mint** | `on_timeout_packet.rs` | Refund on timeout (mint back to sender) |
| **Set Authority** | `initialize.rs` | Transfer mint authority to IFT PDA |
| **Create ATA** | `ift_mint.rs` | Create receiver's token account if needed |

### Operations Intentionally Not Used

| Operation | Reason |
|-----------|--------|
| **Transfer** | Burn-and-mint eliminates need for escrow transfers |
| **Approve/Revoke Delegate** | Users burn directly with their signature |
| **Freeze/Thaw** | No escrow accounts to freeze |
| **Close Account** | Token accounts remain for future transfers |
| **Sync Native (WSOL)** | See WSOL section below |
| **Create Mint** | Mint must exist before IFT initialization |

### Why No WSOL Support

WSOL (Wrapped SOL) is intentionally not supported because:

1. **Semantic Mismatch**: IFT assumes the same logical token exists on both chains. SOL on Solana ≠ any token on Ethereum.

2. **Cannot Burn Native Currency**: You cannot truly "burn" SOL - it would need to be escrowed, which contradicts the burn-and-mint pattern.

3. **Correct Solution**: Use ICS-20 (lock/unlock) for native currency bridging:
   ```
   ICS-20: Lock SOL → Mint wrapped-SOL on destination
   IFT:    Burn USDC → Mint USDC on destination
   ```

## Implementation Details

### Account Structure

```rust
// IFT App State - one per token
pub struct IFTAppState {
    pub version: AccountVersion,
    pub bump: u8,
    pub mint: Pubkey,                    // Token mint address
    pub mint_authority_bump: u8,         // PDA bump for signing
    pub access_manager: Pubkey,          // Role-based access control
    pub gmp_program: Pubkey,             // ICS27-GMP program
    pub _reserved: [u8; 128],
}

// IFT Bridge - one per destination chain
pub struct IFTBridge {
    pub version: AccountVersion,
    pub bump: u8,
    pub mint: Pubkey,
    pub client_id: String,               // IBC client (max 64)
    pub counterparty_ift_address: String,// IFT contract on destination (max 128)
    pub counterparty_chain_type: CounterpartyChainType,
    pub active: bool,
    pub _reserved: [u8; 64],
}

// Pending Transfer - tracks in-flight transfers for refunds
pub struct PendingTransfer {
    pub version: AccountVersion,
    pub bump: u8,
    pub mint: Pubkey,
    pub client_id: String,               // max 64
    pub sequence: u64,
    pub sender: Pubkey,                  // Original sender (for refunds)
    pub amount: u64,                     // Amount transferred (for refunds)
    pub timestamp: i64,
    pub _reserved: [u8; 32],
}
```

### PDA Derivation

```rust
// App State PDA
seeds = [b"ift_app_state", mint.as_ref()]

// Bridge PDA
seeds = [b"ift_bridge", mint.as_ref(), client_id.as_bytes()]

// Mint Authority PDA (signs for mint operations)
seeds = [b"ift_mint_authority", mint.as_ref()]

// Pending Transfer PDA
seeds = [b"pending_transfer", mint.as_ref(), client_id.as_bytes(), &sequence.to_le_bytes()]
```

**Note on Query Methods**: Unlike EVM which requires explicit getters (`getIFTBridge()`, `getPendingTransfer()`), Solana account data is directly readable via RPC. Clients derive PDAs and fetch account data directly.

### Cross-Chain Payload Encoding

| Destination | Format |
|-------------|--------|
| EVM | ABI-encoded `iftMint(address,uint256)` |
| Cosmos | Protojson `MsgIFTMint` |
| Solana | Anchor discriminator + pubkey + amount |

### Transfer Flow

```
1. User calls ift_transfer(receiver, amount, client_id, timeout)
   - Default timeout: 900 seconds (15 minutes) if not specified
   - Max timeout: 86400 seconds (24 hours)
2. IFT burns tokens from sender's account
3. IFT constructs mint payload for destination chain type
4. IFT calls GMP.send_call() with empty salt
5. GMP creates IBC packet via router
6. Relayer delivers packet to destination
7. On success: destination IFT mints to receiver
8. On failure/timeout: source IFT mints back to sender (refund)
```

## Security Model

### Admin Instructions

| Instruction | Purpose |
|-------------|---------|
| `initialize` | Set up IFT for an existing token, transfer mint authority to PDA |
| `register_ift_bridge` | Register counterparty IFT contract for a destination chain |
| `remove_ift_bridge` | Deactivate/remove a registered bridge |
| `set_access_manager` | Update access manager program |

### Access Control

| Role | Capability |
|------|------------|
| **Admin** | Register/remove bridges, update access manager |
| **Mint Authority PDA** | Sole authority to mint tokens |
| **GMP Account PDA** | Validates incoming mint requests |
| **Users** | Initiate transfers (burn their own tokens) |

### Key Security Properties

1. **Burn Authorization**: Only token owner can burn (standard SPL token semantics)

2. **Mint Authorization**: Only IFT's mint authority PDA can mint, controlled by incoming GMP calls validated against registered bridge, or refund operations validated by pending transfer records

3. **Empty Salt Requirement**: Salt is hardcoded empty on send and validated empty on receive via GMP account PDA derivation. Prevents unauthorized minting via alternate GMP account PDAs.

4. **GMP Validation**: Incoming mints verify GMP account matches expected PDA derived from counterparty address

5. **Replay Protection**: Pending transfers use sequence numbers, closed after processing

## Integration with ICS27-GMP

IFT uses GMP as pure transport, not for account control:
- **GMP Standard**: GMP Account PDA controls assets
- **IFT Usage**: GMP Account PDA validates sender identity only

IFT maintains its own mint authority and doesn't delegate token control to GMP PDAs.

### Callback Routing

GMP routes timeout/ack callbacks to IFT via implicit sender detection (see [GMP ADR](./solana-ics27-gmp-architecture.md#packet-lifecycle-callbacks) for mechanism details).

IFT implements `on_timeout_packet` and `on_acknowledgement_packet` handlers to refund burned tokens on failure. These handlers accept CPI calls from both Router (direct) and GMP (for forwarded callbacks):

## Performance (TODO: Update post testing)

| Metric | Value |
|--------|-------|
| **Compute Budget** | 400,000 CU for transfer (IFT→GMP→Router CPI chain) |
| **Accounts - transfer** | 18 (including GMP/Router infrastructure) |
| **Accounts - mint** | 12 |
| **Storage per Bridge** | 308 bytes |
| **Storage per Pending** | 198 bytes (reclaimed on completion) |

## Alternatives Considered

### 1. IFT as standalone IBC app vs GMP transport

**Chosen**: IFT uses GMP as transport layer.

**Alternative**: IFT registers its own port directly with Router. Rejected because it duplicates GMP's packet encoding/decoding logic and diverges from Solidity architecture where IFT uses GMP.

### 2. Burn-and-mint vs escrow pattern

**Chosen**: Burn tokens on source, mint on destination.

**Alternative**: Escrow tokens on source (like ICS-20). Rejected because IFT assumes the same logical token exists on both chains with coordinated supply. Escrow adds complexity without benefit for this use case.

### 3. Pending transfer tracking vs stateless refunds

**Chosen**: Store pending transfers on-chain for refund processing.

**Alternative**: Derive refund info from packet data alone. Rejected because packet data doesn't contain sender's token account or original amount after potential fee deductions. On-chain state ensures reliable refunds.

## Limitations

1. **Pre-existing Mint Required**: Token must exist before IFT initialization
2. **Mint Authority Transfer**: Original authority loses control permanently
3. **No WSOL**: Native currency bridging requires ICS-20

## References

- [ICS-20 Fungible Token Transfer](https://github.com/cosmos/ibc/tree/main/spec/app/ics-020-fungible-token-transfer)
- [SPL Token Program](https://spl.solana.com/token)
- [ICS27-GMP ADR](./solana-ics27-gmp-architecture.md)
