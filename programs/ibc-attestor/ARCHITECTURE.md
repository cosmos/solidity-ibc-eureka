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
- **Signer** (`src/signer.rs`): k256 cryptographic signing
- **gRPC Server** (`src/server.rs`): API, used primarily by aggregator

#### Key Features

- **Modular Design**: Adapter pattern enables easy addition of new chains
- **Serde Encoding**: JSON serialization for cross-language compatibility

#### Attestation Types

From an RPC perspective we only have one attestation type. The proto definition for this type includes on optional field, timestamp. Conceptually there are, however, two attestation types: state attestations and height attestations.

**State Attestations:**
- Attest to blockchain state at specific block heights
- Include height and timestamp information
- Used for IBC client updates

**Packet Attestations:**
- Validate packet commitments existence by querying a local/trusted node
- Concurrent validation of multiple packets per request

#### Signing attestations

The algorithm for signing attestations is the same for any type that implements the `Signable` trait. What differs between signatures is which data is included in the `to_serde_encoded_bytes`. In the attestor service we use explicit `Unsigned` types that implement the `Signable` trait. This ensures that new adapters do not have to care about the details of signing.

One current limitation to note here is that the hashing of the data before signing cannot easily be shared across applications in the attestor stack. This should be refactored to reduce implicit coupling between signing and signature validation.

#### Writing new adapters

To extend the attestor with a new adapter, you will need to:
- Create a new feature-flagging `adapter_impl/<adapter_name>` model
- Implement a config based client instantiation
- Implement the `AttestationAdapter` trait
- Extend the `bin` to accept and run a server for the new adapter.

We have decided to refactor the use of feature flags in the near future so that they are composable. This is more closely aligned with rust best practices.

### 2. Aggregator Service (`programs/sig-aggregator`)

The Aggregator Service collects attestations from multiple attestors and enforces quorum requirements.

#### Core Functionality

- **Quorum Validation**: Requires m-of-n signatures (typically 2/3 threshold)
- **Concurrent Queries**: Simultaneous requests to all configured attestors
- **Timeout Handling**: Graceful degradation when attestors are unavailable
- **Caching**: In-memory cache for recently aggregated attestations
- **Height Selection**: Returns highest common height meeting quorum

### 3. Light Client

The light client verifies aggregated attestations and integrates with IBC protocol.

In its current form the light client copies wherever possible the style and implementation of the ethereum light client. The ethereum light client was implemented with the ethereum spec in mind in order to improve the auditablity of the code. Unfortunately this design decision leaked unknownly into the design of the attestor light client. In the future our attestor client should focus leveraging rust's type system in its design.

The primary difference between the light client library and the CosmWasm program is that the former is functional, operating on arguments, while the latter is stateful. This separation makes testing the library much easier. Testing the program requires more complex integration tests that leverage CosmWasm testing utils.

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

#### CosmWasm state management

The attestor 08-wasm client manages state in a similar way to as the ethereum light client. The primary difference is that the attestor client can and should have multiple states. This is because there is no guarantee around request ordinality. In other words, packet attestations at height 100 may be received by the light client after an attestation at a later height. One complication of storing multiple heights is that we need to ensure that the following be true:

```
... < timestamp(height - 1) < timestamp(height) < timestamp(height + 1) < ...
```

This implies that whenever we add a new height we need to know the nearest neighbours to assert the above formula. The naive solution is, upon insertion, to iterate over all existing heights to find the nearest neighbour. This means lookups become incrementally more expensive with time at a rate on O(n).

The chosen solution to this takes inspiration from ibc-go. We use a second state map to store all the `(height, timestamp)` pairs in ascending order. This can make use of binary search to reduce the increase in lookup times to O(log(n)). We also leverage CosmWasm methods to do the binary search lookup.

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
