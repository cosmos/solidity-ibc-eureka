# Attestor Packet Membership

This package provides packet membership verification functionality for IBC attestor chains.

## Overview

The attestor packet membership package contains utilities for verifying that IBC packet commitments exist within packet attestations. Attested data is represented as a list of `PacketCompact` structs, each containing a commitment path and commitment value.

## Key Components

- **`PacketCommitments`**: Wrapper over a vector of packet commitments with path and commitment values:
  - Each packet is represented as a `PacketCompact` with `path` and `commitment` hashes
  - `new(Vec<PacketCompact>)` to create from packet commitments
  - `iterate()` to iterate over packet commitments
  - `commitments()` to iterate over commitment values only
  - `to_abi_bytes()` and `from_abi_bytes(&[u8])` for encoding/decoding
- **`verify_packet_membership`**: `fn verify_packet_membership(proof: PacketCommitments, value: Vec<u8>) -> Result<(), PacketAttestationError>`
- **`PacketAttestationError`**: Error type for membership verification failures

## Functionality

### Packet Membership Verification

`verify_packet_membership` checks that a given `value` (packet commitment bytes) exists among the attested commitments in `proof: PacketCommitments`. Each commitment is attested as a tuple of `(path_hash, commitment_hash)` where `path_hash` is the commitment path hash and `commitment_hash` is the commitment value. The function verifies that the provided `value` matches one of the commitment values in the attested list.

## Error Handling

On success, the function returns `Ok(())`. If the packet is not found, it returns:

- `PacketAttestationError::VerificiationFailed { reason }`

## Design Philosophy

This implementation prioritizes simplicity over cryptographic complexity. Instead of using merkle proofs or other advanced verification mechanisms, it performs straightforward byte comparison over attested packet commitment paths and values. This approach is suitable for attestor-based IBC implementations where trust is established through attestation rather than cryptographic proofs.
