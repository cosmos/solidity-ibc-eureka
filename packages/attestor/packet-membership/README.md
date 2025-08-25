# Attestor Packet Membership

This package provides packet membership verification functionality for IBC attestor chains.

## Overview

The attestor packet membership package contains utilities for verifying that IBC packets exist within packet attestations. It provides a simple, JSON-based approach to prove packet membership without requiring complex cryptographic proofs.

## Key Components

- **`verify_packet_membership`**: Core function that verifies whether a specific packet exists in a packet attestation proof
- **`PacketAttestationError`**: Error types for packet verification failures

## Functionality

### Packet Membership Verification

The `verify_packet_membership` function takes two parameters:
- `proof`: A JSON-serialized vector of packet bytes representing the attestation
- `value`: A JSON-serialized packet to verify membership for

The function returns `Ok(())` if the packet is found in the proof, or an error if:
- The proof cannot be deserialized
- The value cannot be deserialized  
- The packet is not found in the attestation

### Example Usage

```rust
use attestor_packet_membership::verify_packet_membership;

// Create a proof containing multiple packets
let packets = vec![b"packet1".to_vec(), b"packet2".to_vec(), b"packet3".to_vec()];
let proof = serde_json::to_vec(&packets).unwrap();

// Verify that a specific packet exists in the proof
let value = serde_json::to_vec(b"packet2".as_slice()).unwrap();
let result = verify_packet_membership(proof, value);

assert!(result.is_ok());
```

## Error Handling

The package defines `PacketAttestationError` with the following variants:

- `SerdeDeserializationError`: When JSON deserialization fails
- `VerificationFailed`: When the packet is not found in the attestation

## Design Philosophy

This implementation prioritizes simplicity over cryptographic complexity. Instead of using merkle proofs or other advanced verification mechanisms, it uses straightforward JSON serialization and direct comparison to verify packet membership. This approach is suitable for attestor-based IBC implementations where trust is established through attestation rather than cryptographic proofs.
