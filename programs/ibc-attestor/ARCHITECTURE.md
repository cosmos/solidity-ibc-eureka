# IBC Attestation Architecture

## Overview

The IBC Attestation system is a modular, multisig-based light client solution designed to enable rapid cross-chain integration with Ethereum L2s (Arbitrum, Base), Solana and any other network that can be attested to by an off-chain actor. It provides cryptographically signed attestations, from a set of trusted attestors, of blockchain state that can be used for secure, time-sensitive cross-chain communication.

**Key Features:**
- **Rapid Integration**: Deploy connections to supported ecosystems quickly
- **Multi-chain Support**: Extensible adapter pattern for different ecosystems
- **Security**: m-of-n signature verification with trusted attestor set
- **Performance**: Concurrent processing and caching for low-latency operations

## System Architecture

The system consists of three main components working together:

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Attestor  │───▶│ Aggregator  │───▶│Light Client │
│   Service   │    │   Service   │    │             │
└─────────────┘    └─────────────┘    └─────────────┘
       │                    │                  │
   Signs state &        Collects m-of-n    Verifies sigs &
   packet data         signatures with     updates IBC state
                       quorum validation
```

### 1. Attestor Service (`programs/ibc-attestor`)

The Attestor Service monitors blockchain networks and provides cryptographically signed attestations of their state.

#### Core Components

- **Adapter Client** (`src/adapter_client.rs`): Generic interface for blockchain interaction
- **Chain Adapters** (`src/adapter_impls/`): Network-specific implementations
- **Signer** (`src/signer.rs`): secp256k1 cryptographic signing
- **gRPC Server** (`src/server.rs`): API, used primarily by aggregator

#### Attestation Types

**State Attestations:**
- Attest to blockchain state at specific block heights
- Include height and timestamp information
- Used for IBC client updates

**Packet Attestations:**
- Validate packet commitments existence by querying a local/trusted node
- Concurrent validation of multiple packets per request

#### Key Features

- **Modular Design**: Adapter pattern enables easy addition of new chains
- **Serde Encoding**: JSON serialization for cross-language compatibility

### 2. Aggregator Service (`programs/sig-aggregator`)

The Aggregator Service collects attestations from multiple attestors and enforces quorum requirements.

#### Core Functionality

- **Quorum Validation**: Requires m-of-n signatures (typically 2/3 threshold)
- **Concurrent Queries**: Simultaneous requests to all configured attestors
- **Timeout Handling**: Graceful degradation when attestors are unavailable
- **Caching**: In-memory cache for recently aggregated attestations
- **Height Selection**: Returns highest common height meeting quorum

### 3. Light Client

The Light Client verifies aggregated attestations and integrates with IBC protocol.

#### Components

- **Core Light Client** (`packages/attestor/light-client/`):
  - Stateless implementation of light client logic
  - Client state logic
  - Consensus state logic
  - Signature verification
- **CosmWasm Integration** (`programs/cw-ics08-wasm-attestor`):
  - Smart contract deployable to ibc-go's 08-wasm module
  - Implements the Light Client Module interface
  - Store and retrieval of state
- **Packet Membership** (`packages/attestor/packet-membership/`):
  - Packet membership types and logic
  - Verify packet inclusion in attested data (i.e., not from native blockchain proof)

#### Verification Process

1. **Signature Verification**: Validate m-of-n signatures from trusted attestor set
2. **Height Validation**: Ensure attestation height meets minimum requirements  
3. **State Updates**: Update consensus state with new blockchain data
4. **Packet Verification**: Verify packet existence through attestor signatures

### Key Design Principles

- **Modularity**: Components should be loosely coupled and independently testable
- **Error Handling**: Graceful degradation and detailed error reporting
- **Performance**: Concurrent processing where possible
- **Simplicity**: Avoid over-engineering, focus on core functionality

## 📋 Future/Unplanned/Not in current scope

- **Misbehaviour Detection**: Automated pause mechanisms (not implemented yet, but coming)
- **Monitoring System**: For L2s: Reorg and outage detection (no concrete timeline)
- **Sequencer Key Verification**: Additional signature validation (not planned)

## Security Model

### Trust Assumptions

- **Attestor Set**: Trust in m-of-n configured attestors
- **No Native Proofs**: System relies on attestor signatures rather than blockchain-native proofs
- **Simplified Model**: Focus on signature verification over complex fraud proofs

### Risk Mitigation

- **Diverse Attestor Set**: Multiple independent operators reduce collusion risk
- **Signature Verification**: Cryptographic proof of attestor participation
- **Quorum Requirements**: Multiple signatures required for validity
- **Future Additions**: If/when needed, additional security layers and verifications can be added

### Known Limitations

- **No Monitoring**: Currently no automated detection of reorgs or outages
- **Trust-based**: Security depends on attestor honesty and availability
- **No Slashing**: No economic penalties for malicious behavior


## Technical Reference

### Key Files

- `programs/ibc-attestor/src/attestor.rs`: Core attestor service logic
- `programs/sig-aggregator/src/aggregator.rs`: Signature aggregation logic  
- `packages/attestor/light-client/src/verify.rs`: Signature verification
- `packages/attestor/packet-membership/src/verify_packet_membership.rs`: Packet validation