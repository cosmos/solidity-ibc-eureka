# `CosmWasm` ICS-08 Wasm Attestor Client

This `CosmWasm` smart contract implements the ICS-08 Wasm light client interface for generic attestation clients.

## Overview

This contract provides the `CosmWasm` wrapper around a generic attestation client, enabling IBC connections between Cosmos chains and various blockchain clients. It implements the standard IBC light client interface through `CosmWasm` entry points.

## Entry Points

### Instantiate
- Initializes the attestor client with initial client state and consensus state

### Query
- **`verify_client_message`**: Validates client messages (headers)
- **`check_for_misbehaviour`**: Checks for misbehavior in client messages (TODO)
- **`timestamp_at_height`**: Returns timestamp at a given height
- **status**: Returns client status (Active/Frozen/Expired)

### Sudo
- **`update_state`**: Updates the client and consensus state with new blockchain data
- **`verify_membership`**: Verifies membership proofs (TODO in initial implementation)
- **`verify_non_membership`**: Verifies non-membership proofs (TODO in initial implementation)
- **`update_state_on_misbehaviour`**: Handles misbehavior by freezing client (TODO)

## Initial Implementation Scope

The initial implementation focuses on:
1. Basic client instantiation
2. Header verification (`verify_client_message`)
3. State updates (`update_state`)
4. Simple status queries

Advanced features like membership proof verification will be added in subsequent iterations.
