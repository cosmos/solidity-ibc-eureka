# ADR: ICS27 General Message Passing (GMP) Architecture for Solana

**Status**: Implemented
**Date**: 2025-09-18
**Last Updated**: 2025-11-07

## Executive Summary

We implement ICS27 GMP for Solana using a **flattened single-program architecture** that preserves CPI depth for target applications while maintaining cross-chain account determinism through Program Derived Addresses (PDAs).

## Problem

The ICS27 General Message Passing protocol enables cross-chain smart contract execution. The Ethereum reference implementation uses a two-contract architecture:

```
User → GMP Contract → Account Contract → Target Contract → Unlimited subcalls
```

However, Solana's constraints make this architecture impractical:

| Constraint              | Ethereum             | Solana           | Impact                                           |
| ----------------------- | -------------------- | ---------------- | ------------------------------------------------ |
| **Call Depth**          | Unlimited            | 4 CPI levels max | Would consume 3/4 levels before target execution |
| **Account Model**       | Deployable contracts | PDAs only        | Cannot deploy per-user contracts                 |
| **Account Declaration** | Dynamic              | All upfront      | Must know all accounts before execution          |
| **Address Format**      | String (0x...)       | 32-byte Pubkey   | Type conversion required                         |

## Solution

### Architectural Innovation: Flattened Single-Program Design

Instead of separate GMP and Account contracts, we merge them into a single program that preserves precious CPI depth:

```
Ethereum:  Router → GMP → Account → Target → ... (3 levels used)
Solana:    Router → GMP+Account → Target → ...   (2 levels used)
```

This gives target programs 2 additional CPI levels to work with - critical for complex DeFi operations.

### Key Design Decisions

1. **PDA as Cross-Chain Identity**

   - Each Cosmos user gets a deterministic PDA from `AccountIdentifier(client_id, sender, salt)`
   - Uses Borsh serialization + SHA256 hashing for collision-resistant identifier derivation
   - PDA has no account data - only used for signing via `invoke_signed`
   - Can own SPL tokens and other assets (PDA acts as authority for token accounts)
   - Zero rent cost - address exists deterministically, no account creation needed

2. **Relayer-Computed Accounts**

   - Relayer derives `gmp_account_pda` and `target_program`
   - Sender only provides target-specific accounts
   - Simplifies sender complexity while maintaining security

3. **Protobuf Payload Format**
   - Cross-chain compatible encoding
   - Contains target program ID, accounts, and instruction data
   - Relayer parses to extract required accounts

## Implementation Details

### PDA Derivation

Each Cosmos user gets a deterministic Solana address derived from an `AccountIdentifier`:

```rust
// AccountIdentifier uniquely identifies a cross-chain account
struct AccountIdentifier {
    client_id: String,  // e.g., "07-tendermint-0"
    sender: String,     // Cosmos address
    salt: Vec<u8>,      // User-provided uniqueness
}

// The identifier is Borsh-serialized and hashed to produce a 32-byte seed.
// Borsh encoding prevents collision attacks by length-prefixing each field:
// - String: u32 length (little-endian) + UTF-8 bytes
// - Vec<u8>: u32 length (little-endian) + bytes
let identifier_hash = sha256(borsh::to_vec(&AccountIdentifier {
    client_id,
    sender,
    salt,
}));

// PDA seeds: [b"gmp_account", identifier_hash]
let (account_pda, bump) = Pubkey::find_program_address(&[
    b"gmp_account",
    &identifier_hash,
], &gmp_program_id);
```

**Security Note**: Using Borsh serialization prevents hash collision attacks where malicious inputs with different field boundaries could produce identical hashes. For example, without length-prefixing, `client_id="ab", sender="cd"` would hash the same as `client_id="abc", sender="d"`.

### Account Layout and Execution Flow

The relayer constructs the transaction with carefully ordered accounts:

```rust
// Account ordering (critical for proper execution):
// [0] gmp_account_pda   - Relayer computes from seeds
// [1] target_program      - Relayer extracts from GMPPacketData.receiver
// [2+] target accounts    - Sender provides in GMPSolanaPayload.accounts

// The GMP program executes target with PDA as signer:
invoke_signed(
    &target_instruction,
    &target_accounts,
    &[&[b"gmp_account", &identifier_hash, &[bump]]]
)?;
```

**Critical Design: Conditional Fee Payer Injection**

Solana PDAs with data cannot pay for account creation. We solve this with a **configurable payer injection** mechanism controlled by the optional `payer_position` field in `GMPSolanaPayload`:

- **`payer_position` not set**: No injection (for programs that don't create accounts, e.g., SPL Token Transfer)
- **`payer_position = N`**: Inject at index N (0-indexed array position)

This allows:

- GMP PDA to sign for operations via `invoke_signed`
- Relayer to pay for new account rent when needed
- Target programs to create accounts as needed
- Preserves exact account layouts for programs with fixed schemas
- Full flexibility for sender to control account ordering

### Payload Structure

The payload uses Protobuf for cross-chain compatibility:

```proto
message SolanaAccountMeta {
  bytes pubkey = 1;         // Account public key (32 bytes)
  bool is_signer = 2;       // Should this account sign at CPI instruction level?
  bool is_writable = 3;     // Will this account be modified?
}

message GMPSolanaPayload {
  repeated SolanaAccountMeta accounts = 1; // Accounts needed by target
  bytes data = 2;                          // Instruction data
  optional uint32 payer_position = 3;      // Position to inject relayer as payer
}

message GMPPacketData {
  string sender = 1;      // Original sender address
  string receiver = 2;    // Target program ID
  bytes salt = 3;         // Account uniqueness
  bytes payload = 4;      // GMPSolanaPayload (protobuf)
  string memo = 5;        // Optional memo field
}
```

**Key Design**: Sender provides only target-specific accounts. Relayer adds protocol accounts (gmp_account_pda, target_program) automatically.

**Note**: The `client_id` field was removed from `GMPPacketData` as it's available in the IBC packet metadata.

### Signing Architecture: Two-Level Model

Solana has two distinct levels of signing that are critical to understand:

1. **Transaction Level**: Requires private key signatures before transaction submission
2. **Instruction Level**: PDAs sign via `invoke_signed` using seed-based authority during CPI

The `is_signer` field in `SolanaAccountMeta` indicates whether an account should be a signer **at the CPI instruction level** (not at the transaction level). This simplified design works because:

**For cross-chain calls from Cosmos**:

- Cosmos users don't have Solana private keys, so transaction-level signing is not applicable
- The ICS27 GMP Account PDA represents the user and signs via `invoke_signed`
- All payload accounts are marked `is_signer: false` at transaction level
- The GMP program marks the gmp account PDA as a signer when making the CPI call

**Account Signing Behavior**:

- `is_signer: false` → Account does not sign (most accounts: data accounts, programs, system accounts)
- `is_signer: true` → PDA signs via `invoke_signed` during CPI (ICS27 GMP Account PDA)

This keeps the architecture simple while correctly modeling how accounts sign in cross-chain scenarios.

## Example 1: Counter Application

Our e2e tests demonstrate the architecture with a counter application where Cosmos users can increment counters on Solana:

### Counter App Design

```rust
// Solana counter program maintains per-user counters
pub struct UserCounter {
    pub user: Pubkey,        // User identifier
    pub count: u64,          // Current counter value
    pub increments: u64,     // Total increment operations
    pub last_updated: i64,   // Timestamp of last update
}
```

### Cross-Chain Flow

```go
// 1. Cosmos user constructs increment instruction
// Note: Only the amount is in instruction data
// The user authority (ICS27 GMP Account PDA) is passed as an account, not in data
incrementData := []byte{
    INSTRUCTION_INCREMENT,  // Discriminator (8 bytes)
    amount,                 // Increment amount (8 bytes, little-endian u64)
}

// 2. User provides only target-specific accounts
// Note: payer_position = 3 tells GMP program to inject relayer at index 3
payerPosition := uint32(3)
gmpSolanaPayload := &GMPSolanaPayload{
    Data: incrementData,
    Accounts: []*SolanaAccountMeta{
        {counterAppState, false, true},   // [0] app_state (not a signer, writable)
        {userCounterPDA, false, true},    // [1] user_counter (not a signer, writable)
        {ics27AccountPDA, true, false},   // [2] user_authority (PDA signer, read-only)
        // [3] payer will be injected here by GMP program
        {systemProgram, false, false},    // [4] system_program (not a signer, read-only)
    },
    PayerPosition: &payerPosition,  // Inject relayer at index 3
}

// 3. Send via IBC as GMPPacketData
msg := &MsgSendCall{
    Sender:   cosmosUser,
    Receiver: counterProgramID.String(),  // Target program ID
    Payload:  proto.Marshal(gmpSolanaPayload),
    Salt:     []byte{},  // Optional uniqueness
}
```

### Relayer Processing

The relayer automatically adds protocol accounts and handles payer injection:

```rust
// Relayer adds protocol accounts at the beginning:
// [0] gmp_account_pda   - Derived from Borsh-hashed AccountIdentifier
// [1] target_program    - From GMPPacketData.receiver
// [2+] user accounts    - From GMPSolanaPayload.accounts
// [N] payer (injected)  - Injected at payer_position if specified

let gmp_account_pda = derive_gmp_pda(client_id, sender, salt);  // Uses Borsh + SHA256
accounts.insert(0, AccountMeta {
    pubkey: gmp_account_pda,
    is_signer: false,   // No keypair at transaction level
    is_writable: false  // readonly - stateless, no account creation
});
accounts.insert(1, AccountMeta {
    pubkey: counter_program_id,
    is_signer: false,
    is_writable: false
});

// Parse payload to extract user's accounts
let gmp_solana_payload = GMPSolanaPayload::decode(gmp_packet.payload)?;
for account in gmp_solana_payload.accounts {
    accounts.push(AccountMeta {
        pubkey: Pubkey::try_from(account.pubkey)?,
        is_signer: false,  // All payload accounts are non-signers at transaction level
        is_writable: account.is_writable,
    });
}

// Inject payer at specified position if payer_position is set
if let Some(position) = gmp_solana_payload.payer_position {
    accounts.insert(position, AccountMeta {
        pubkey: relayer_keypair.pubkey(),
        is_signer: true,   // Relayer signs to pay for rent
        is_writable: true,
    });
}
```

The GMP program then marks the GMP Account PDA as a signer at CPI instruction level via `invoke_signed`.

### Result

- Each Cosmos user gets their own counter via deterministic user counter PDA (derived from ICS27 GMP Account PDA)
- Multiple users can have independent counters
- **Security**: Only the ICS27 GMP Account PDA can increment its own counter (enforced by `user_authority: Signer` constraint)
- GMP program signs as the GMP Account PDA via `invoke_signed` during CPI
- Counter increments atomically with proper access control

This demonstrates how complex cross-chain operations work with minimal sender complexity while maintaining strong security guarantees.

## Example 2: SPL Token Transfer

A more complex example showing cross-chain token transfers:

### SPL Transfer Flow

```go
// 1. Cosmos user wants to transfer USDC owned by their ICS27 Account PDA
// The ICS27 PDA was previously funded and owns SPL tokens

// 2. Build SPL transfer instruction
transferInstruction := token.NewTransferInstruction(
    1_000_000,           // 1 USDC (6 decimals)
    sourceTokenAccount,  // Token account owned by ICS27 PDA
    destTokenAccount,    // Recipient's token account
    ics27AccountPDA,     // Authority (will be signed by GMP)
).Build()

// 3. Create GMPSolanaPayload with required accounts
// Note: PayerPosition is NOT set because SPL Transfer doesn't create accounts
gmpSolanaPayload := &GMPSolanaPayload{
    Data: transferInstruction.Data(),
    Accounts: []*SolanaAccountMeta{
        {sourceTokenAccount, false, true},  // Source (not a signer, writable)
        {destTokenAccount, false, true},    // Destination (not a signer, writable)
        {ics27AccountPDA, true, false},     // Authority (PDA signer, read-only)
    },
    // PayerPosition: nil - no payer injection needed for SPL transfers
}

// 4. Send as GMP packet
msg := &MsgSendCall{
    Sender:   cosmosUser,
    Receiver: SPL_TOKEN_PROGRAM_ID.String(),  // Target program ID
    Payload:  proto.Marshal(gmpSolanaPayload),
    Salt:     userSalt,  // Same salt to get same ICS27 PDA
}
```

### Key Points

1. **PDA as Token Owner**: The ICS27 Account PDA can be the authority/owner of SPL token accounts
2. **Authority Signing**: GMP program uses `invoke_signed` to sign as the PDA
3. **Deterministic Addressing**: Same user + salt always gets same PDA
4. **Composability**: Works with any SPL token

### Relayer Handling

```rust
// Relayer adds the same protocol accounts as before:
// [0] gmp_account_pda - Derived from (client_id, cosmosUser, salt)
// [1] spl_token_program - From GMPPacketData.receiver
// [2+] token accounts   - From GMPSolanaPayload.accounts

// The ICS27 PDA signs for the transfer using Borsh-hashed identifier
invoke_signed(
    &spl_transfer_instruction,
    &[source_account, dest_account, authority],
    &[&[b"gmp_account", &identifier_hash, &[bump]]]
)?;
```

This example shows how ICS27 enables complex DeFi operations across chains while maintaining the same simple interface for users.

## Address Lookup Tables (ALT) Optimization

### The Transaction Size Challenge

Solana imposes a strict 1232-byte limit on transaction size. A typical GMP transaction requires:

- 10 infrastructure accounts (router, GMP program, light client, state PDAs, etc.)
- Target-specific accounts (varies by application)
- IBC proof data
- Instruction data

Without optimization, each account consumes 32 bytes in the transaction. This leaves limited room for IBC proofs.

### ALT Solution

Address Lookup Tables allow referencing accounts by 1-byte indices instead of 32-byte pubkeys:

```rust
// ALT Creation (one-time setup)
// These are common accounts that appear in every IBC GMP transaction
let alt_accounts = vec![
    system_program_id,        // Index 0
    ics26_router_program,     // Index 1
    ics07_tendermint_program, // Index 2
    ics27_gmp_program,        // Index 3 (ibc_app_program)
    router_state_pda,         // Index 4
    relayer_fee_payer,        // Index 5
    ibc_app_pda,              // Index 6 (port-specific: "gmp" port)
    gmp_app_state_pda,        // Index 7
    client_pda,               // Index 8 (e.g., "solclient-0")
    client_state_pda,         // Index 9 (ICS07 client state for source chain)
];

// Transaction uses 1-byte indices instead of 32-byte pubkeys
let v0_message = v0::Message::try_compile(
    &fee_payer,
    &[instruction],
    &[address_lookup_table],  // Pass ALT
    blockhash,
)?;
```

### Impact

Address Lookup Tables significantly reduce transaction size by replacing 32-byte pubkeys with 1-byte indices for infrastructure accounts. This optimization is critical for fitting IBC proofs within Solana's 1232-byte transaction limit.

## Relayer Architecture

The relayer acts as a smart intermediary that:

1. **Observes** IBC packets from Cosmos chains
2. **Derives** protocol accounts (`gmp_account_pda` from packet data)
3. **Extracts** target accounts from protobuf payload
4. **Constructs** complete Solana transaction with all accounts
5. **Submits** to Solana using ALT for size optimization

This design keeps sender complexity minimal while maintaining security - senders don't need to understand Solana PDAs or account derivation.

### Port-Specific Logic in Relayer

Currently, the relayer contains conditional logic for the GMP port:

```rust
fn extract_payload_accounts(
    payload: &Payload,
    port_id: &str,
    source_client: &str,
    existing_accounts: &[AccountMeta],
) -> Result<Vec<AccountMeta>> {
    // Conditional logic for GMP port
    if port_id == GMP_PORT_ID && payload.encoding == PROTOBUF_ENCODING {
        // Parse GMPPacketData
        let gmp_packet = GmpPacketData::decode(payload.value)?;

        // Derive gmp_account_pda (GMP-specific)
        let gmp_account_pda = derive_gmp_account(...);
        accounts.push(gmp_account_pda);

        // Extract target program and accounts
        let gmp_solana_payload = GMPSolanaPayload::decode(gmp_packet.payload)?;
        // ... add accounts
    } else {
        // Other ports would need their own logic
        return Err("Unsupported port");
    }
}
```

## Security Model

- **Account Control**: Only GMP program can sign via `invoke_signed` - users cannot directly control PDAs
- **Replay Protection**: Per-account nonces prevent replay attacks
- **Deterministic Addressing**: Same seeds always produce same PDA - no address spoofing
- **Equivalent to Ethereum**: Same security properties as `onlyICS27` modifier

## Performance

- **Transaction Size**: Stays within 1232-byte limit using ALT optimization
- **Compute Units**: Well below 1.4M limit
- **CPI Depth**: 2 levels available for target programs

## Trade-offs

### Accepted Limitations

- **CPI Depth**: 2 levels for target programs (vs unlimited on Ethereum)
- **Account Pre-declaration**: All accounts must be known upfront
- **Transaction Size**: 1232-byte limit constrains complexity

### Mitigation Strategies

- **Address Lookup Tables**: Reduce account size from 32 bytes to 1 byte
- **Relayer Intelligence**: Relayer derives protocol accounts, sender only provides target accounts
- **Protobuf Encoding**: Efficient cross-chain serialization

## Conclusion

This architecture successfully adapts ICS27 GMP to Solana's constraints by flattening the contract hierarchy and leveraging PDAs for deterministic addressing. The design preserves maximum CPI depth for target applications while maintaining security equivalence with Ethereum.

The key innovation is the split of responsibilities: senders provide minimal information (just target accounts), while the relayer handles all protocol complexity. This makes cross-chain calls from Cosmos to Solana as simple as possible for end users.

## References

- [Solana Program Derived Addresses](https://docs.solana.com/developing/programming-model/calling-between-programs#program-derived-addresses)
- [ICS27 Interchain Accounts Specification](https://github.com/cosmos/ibc/tree/main/spec/app/ics-027-interchain-accounts)
