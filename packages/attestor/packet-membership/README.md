# Attestor Packet Membership

This package provides packet membership verification functionality for IBC attestor chains.

## Overview

The attestor packet membership package contains utilities for verifying that IBC packet commitments exist within packet attestations. Attested data is represented as an ABI-encoded `bytes32[]`, wrapped by the `Packets` type for ergonomic encoding/decoding and iteration.

## Key Components

- **`Packets`**: Wrapper over a vector of 32-byte commitments with helpers:
  - `new(Vec<T>)` where `T: Into<FixedBytes<32>>`
  - `packets()` iterator
  - `to_abi_bytes()` and `from_abi_bytes(&[u8])`
- **`verify_packet_membership`**: `fn verify_packet_membership(proof: Packets, value: Vec<u8>) -> Result<(), PacketAttestationError>`
- **`PacketAttestationError`**: Error type for membership verification failures

## Functionality

### Packet Membership Verification

`verify_packet_membership` checks that a given `value` (packet commitment bytes) exists among the attested commitments in `proof: Packets`. The commitments are 32-byte values (Solidity `bytes32`) and can be encoded/decoded via the `Packets` helpers.

## Error Handling

On success, the function returns `Ok(())`. If the packet is not found, it returns:

- `PacketAttestationError::VerificiationFailed { reason }`

## Design Philosophy

This implementation prioritizes simplicity over cryptographic complexity. Instead of using merkle proofs or other advanced verification mechanisms, it performs straightforward byte comparison over attested `bytes32[]` commitments. This approach is suitable for attestor-based IBC implementations where trust is established through attestation rather than cryptographic proofs.
