# Attestation Light Client

An IBC v2 light client for Solana that verifies counterparty chain state through off-chain attestor signatures rather than on-chain consensus proofs (e.g. Tendermint BFT or SP1 zero-knowledge proofs).

## Trust Model

Security relies on an m-of-n set of trusted attestors — off-chain agents that independently observe the counterparty chain and produce secp256k1 ECDSA signatures over its state. The client accepts a state update only when at least `min_required_sigs` distinct trusted attestors have signed the same attestation data.

This trades the trust-minimized guarantees of a full consensus proof for lower verification cost and simpler integration. The trust assumption is that fewer than `min_required_sigs` attestors are compromised at any given time.

## Actors

| Actor | Role |
|-------|------|
| **Admin** | Deploys and initializes the client with the attestor set and quorum threshold |
| **Attestors** | Off-chain agents that watch the counterparty chain and sign attestations over its state |
| **Relayer** | Collects attestor signatures and submits them on-chain via `update_client` (requires `RELAYER_ROLE`) |
| **ICS-26 Router** | Calls `verify_membership` and `verify_non_membership` via CPI during packet processing |

## Instructions

### `initialize`

Creates the client with its initial configuration:

- **Client state** — client ID, attestor Ethereum addresses, quorum threshold (`min_required_sigs`), initial height
- **Consensus state** — height and timestamp at the initial height
- **App state** — reference to the external access manager program

Validation rejects empty client IDs, zero heights/timestamps, empty attestor sets, duplicate attestors and invalid quorum values (zero or exceeding the attestor count).

All three accounts are PDAs owned by the program.

### `update_client`

Advances the client to a new height. The relayer submits a Borsh-serialized `MembershipProof` containing ABI-encoded `StateAttestation` data and the collected signatures.

The instruction:

1. Checks `RELAYER_ROLE` via the external access manager
2. Rejects updates to a frozen client
3. Deserializes the proof and ABI-decodes the state attestation (`height`, `timestamp`)
4. Verifies signatures against the trusted attestor set
5. Checks the attested height matches the instruction parameter

**Misbehaviour detection** — if a consensus state already exists at the target height with a different timestamp, the client freezes itself and emits a `MisbehaviourDetected` event. The instruction returns `Ok(())` (rather than an error) so the freeze persists; an error would revert the state change on Solana.

If no consensus state exists yet, it creates one and advances `latest_height` if the new height exceeds it.

### `verify_membership`

Called by the ICS-26 router via CPI to prove a packet commitment exists on the counterparty chain.

The instruction:

1. Checks the client is not frozen and a trusted consensus state exists at the requested height
2. Deserializes the proof and ABI-decodes the `PacketAttestation` (height + list of path/commitment pairs)
3. Verifies signatures and height match
4. Finds the packet whose `keccak256(path)` matches the requested path
5. Confirms the commitment matches the expected value (a 32-byte hash)

### `verify_non_membership`

Proves a packet commitment does **not** exist (i.e. has been deleted / receipt written) — used for timeout processing.

Works identically to `verify_membership` except:

- The matched packet's commitment must be all zeros (the sentinel for absence)
- Returns the consensus timestamp via `set_return_data` so the router can perform timeout expiration checks

## Signature Verification

Each 65-byte signature (r ‖ s ‖ v) undergoes:

1. SHA-256 hash of the attestation data (computed once, shared across all signatures)
2. secp256k1 ECDSA recovery to obtain the signer's public key
3. Keccak-256 of the recovered public key → 20-byte Ethereum address
4. Check: address is in the trusted set and has not already been seen (no duplicates)

On Solana this uses the `secp256k1_recover` syscall; in tests it uses the `alloy` crate for native ECDSA recovery.

## Account Layout

```
ClientState (PDA: ["client", client_id])
├── version: AccountVersion
├── client_id: String
├── attestor_addresses: Vec<[u8; 20]>
├── min_required_sigs: u8
├── latest_height: u64
└── is_frozen: bool

ConsensusStateStore (PDA: ["consensus", client_state_key, height_le_bytes])
├── height: u64
└── consensus_state: { height, timestamp }

AppState (PDA: ["app_state"])
├── version: AccountVersion
├── access_manager: Pubkey
└── _reserved: [u8; 256]
```

Each height gets its own `ConsensusStateStore` PDA, so historical states are preserved and individually addressable.

## Proof Encoding

The proof is a two-layer encoding:

1. **Outer envelope** — Borsh-serialized `MembershipProof { attestation_data, signatures }`
2. **Inner attestation data** — ABI-encoded (Solidity-compatible) `StateAttestation` or `PacketAttestation`

The inner layer uses ABI encoding so the same attestation bytes can be verified on both Solidity and Solana without re-encoding.

## End-to-End Flow

#### 1. Initialize

An admin deploys the client on Solana with a set of attestor Ethereum addresses (e.g. 5 attestors), a quorum threshold (e.g. 3 of 5) and the initial height/timestamp of the counterparty chain. This creates three PDAs: `ClientState`, `ConsensusStateStore` at the initial height and `AppState`.

#### 2. Attestors observe the counterparty chain

Off-chain attestors independently watch the counterparty chain. When a new block is finalized at height H, each attestor:

1. Reads the chain state at height H
2. Constructs an ABI-encoded `StateAttestation { height: H, timestamp: T }` or `PacketAttestation { height: H, packets: [...] }`
3. SHA-256 hashes the attestation data
4. Signs the hash with their secp256k1 private key → produces a 65-byte signature

Because they all observe the same canonical chain state and use the same ABI encoding, they all sign identical bytes.

#### 3. Relayer collects and submits

The relayer gathers signatures from enough attestors (≥ `min_required_sigs`), wraps them into a Borsh-serialized `MembershipProof { attestation_data, signatures }` and calls `update_client`. The instruction verifies every signature by recovering the signer's Ethereum address and checking it against the trusted set, then stores the new `ConsensusState` and advances `latest_height` if needed.

Now Solana knows: "the counterparty chain was at height H with timestamp T, and at least m trusted attestors agree."

#### 4. Packet relay (verify_membership)

A user sends a packet on the counterparty chain. The packet commitment gets stored at a path like `ibc/commitments/channel-0/sequence/1`. The attestors sign a `PacketAttestation` containing the path/commitment pairs at height H.

The relayer submits a `recv_packet` to the ICS-26 router on Solana. The router calls `verify_membership` via CPI, which verifies signatures, finds the packet by `keccak256(path)` and confirms the commitment matches. On success the router delivers the packet.

#### 5. Timeout (verify_non_membership)

If the counterparty chain deletes a packet commitment (indicating it was received or timed out), the commitment becomes zero. The relayer proves this absence through `verify_non_membership` — same flow as above but the matched commitment must be all zeros. The instruction returns the consensus timestamp via `set_return_data` so the router can check timeout expiration.

#### 6. Misbehaviour

If the relayer submits an `update_client` for height H but a `ConsensusStateStore` already exists at H with a different timestamp, the client freezes permanently. Once frozen, all instructions reject with `FrozenClientState`.

```
Chain A                    Attestors                 Relayer                    Solana
  │                           │                        │                         │
  │  block at height H        │                        │                         │
  ├──────────────────────────►│                        │                         │
  │                           │  sign(attestation)     │                         │
  │                           ├───────────────────────►│                         │
  │                           │  sig1, sig2, sig3      │                         │
  │                           │                        │  update_client(proof)   │
  │                           │                        ├────────────────────────►│
  │                           │                        │                         │ verify sigs
  │                           │                        │                         │ store consensus
  │                           │                        │                         │
  │  packet committed         │                        │                         │
  ├──────────────────────────►│                        │                         │
  │                           │  sign(packets)         │                         │
  │                           ├───────────────────────►│                         │
  │                           │                        │  recv_packet            │
  │                           │                        ├────────────────────────►│
  │                           │                        │                  router │
  │                           │                        │          CPI ──► verify_membership
  │                           │                        │                         │ verify sigs
  │                           │                        │                         │ match path
  │                           │                        │                         │ check commitment
  │                           │                        │                         │ deliver packet
```
