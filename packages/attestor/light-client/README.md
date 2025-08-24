# Attestor Light Client

This package contains the core attestor light client implementation for IBC.

## Overview

The attestor light client provides IBC client functionality for verifying blockchain state through attestation and facilitating cross-chain communication with Cosmos-based chains.

## Key Components

- **ClientState**: Chain parameters and configuration
- **ConsensusState**: Trusted chain state at specific heights with minimal height and timestamp
- **Header**: Block/state information for updates
- **Verification**: Logic to verify client messages and state transitions
- **Updates**: Logic to update consensus state with new blockchain data

## Initial Implementation Focus

This initial implementation focuses on:
1. Basic client state management
2. Consensus state updates using minimal height and timestamp data
3. Simple verification of state transitions through attestation
4. Integration with IBC verify_client_message and update_state methods

Note: Advanced features like merkle proof verification for membership proofs are not included in the initial implementation.
