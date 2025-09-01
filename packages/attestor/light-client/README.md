# Attestor Light Client

This package contains the core attestor light client implementation for IBC.

## Overview

The attestor light client provides IBC client functionality for verifying blockchain state via attestation and facilitating cross-chain communication with Cosmos-based chains. Attestations consist of ABI-encoded data and raw 65-byte ECDSA signatures; signer addresses are recovered and checked against the trusted attestor set stored in the client state. Timestamps are validated for consistency and monotonicity.

## Key Components

- **ClientState**: Chain parameters and configuration (trusted attestor addresses, signature threshold, latest height, frozen flag)
- **ConsensusState**: Trusted chain state at specific heights with height and timestamp
- **Header**: Block/state update container with `attestation_data` and raw `signatures`
- **verify_attestation**: Address-recovery based cryptographic verification over 65-byte signatures
- **verify::verify_header**: Header verification (attestation + timestamp invariants)
- **update::update_consensus_state**: Applies verified headers and optionally bumps client state height
- **membership::verify_membership**: Verifies packet membership using attested `bytes32[]` commitments

## Initial Implementation Focus

This initial implementation focuses on:
1. Basic client state management
2. Consensus state updates using minimal height and timestamp data
3. Verification of state transitions through attestation (65-byte signatures, address recovery)
4. Packet membership verification against attested `bytes32[]` commitments
5. Integration points for IBC `verify_client_message` and `update_state`

Note: Advanced features like merkle proof verification are not included.

## Attestation Format

- `attestation_data`: ABI-encoded payload. For packet membership, this is an ABI-encoded `PacketAttestation` struct containing packet commitment paths and values.
- `signatures`: Raw 65-byte `(r || s || v)` signatures. Signer addresses are recovered and must exist in `ClientState.attestor_addresses`. Duplicate signatures and insufficient signatures are rejected.

Construct client state with addresses directly:

```rust
use attestor_light_client::client_state::ClientState;
use alloy_primitives::Address;

let attestor_addresses: Vec<Address> = vec![
    Address::from([0x11u8; 20]),
    Address::from([0x22u8; 20]),
];
let client_state = ClientState::new(attestor_addresses, 2, 42);
```
