# IBC Relayer API Architecture

**Location**: [`programs/relayer/`](programs/relayer/) (entry point) | [`packages/relayer/`](packages/relayer/) (implementation) | [`proto/relayer/`](proto/relayer/) (API definition)

The IBC Relayer is a stateless gRPC API service that provides Inter-Blockchain Communication (IBC) relaying functionality. It serves as a transaction generation service, creating unsigned transactions for IBC operations while delegating monitoring, submission, and orchestration responsibilities to callers.

## Architectural Principles

### Separation of Concerns
The architecture deliberately separates transaction generation from operational concerns:

**What the Relayer Does**:
- Generate unsigned transactions for IBC packet life cycle operations
- Generate unsigned transactions for IBC client creation
- Generate unsigned transactions for IBC client updates

**What the Relayer Does NOT Do**:
- Monitor blockchains for new events in the background
- Submit transactions
- Manage private keys or signing
- Maintain relayer state or persistence

### Modular Architecture
The system uses a plugin-based architecture where different modules handle specific blockchain combinations. This allows:
- **Extensibility**: New blockchain types can be added without core changes
- **Isolation**: Each module operates independently
- **Specialization**: Modules can optimize for specific chain characteristics
- **Configuration**: Modules can be enabled/disabled per deployment

## System Architecture

### Service Layer
**Location**: [`programs/relayer/`](programs/relayer/) - Entry point and CLI interface

The top level exposes a standard gRPC service interface with four core operations:

1. **RelayByTx**: Process specific transactions and generate relay operations (IBC packet life cycle)
2. **CreateClient**: Generate light client creation transactions
3. **UpdateClient**: Generate light client update transactions  
4. **Info**: Return chain and module information

### Routing Layer
**Location**: [`packages/relayer/core/`](packages/relayer/core/) - Core routing and builder logic

Requests are routed to appropriate modules based on (source_chain, destination_chain) pairs.

### Module Layer
**Location**: [`packages/relayer/modules/`](packages/relayer/modules/) - Chain-specific implementations

Each module implements the same interface but handles different blockchain combinations, some examples include:
- **Cosmos ↔ Ethereum**: [`cosmos-to-eth/`](packages/relayer/modules/cosmos-to-eth/) - Handles Tendermint/Ethereum with zero-knowledge proofs
- **Cosmos ↔ Cosmos**: [`cosmos-to-cosmos/`](packages/relayer/modules/cosmos-to-cosmos/) - Native IBC between Cosmos SDK chains
- **Ethereum ↔ Cosmos**: [`eth-to-cosmos/`](packages/relayer/modules/eth-to-cosmos/) - Ethereum light client verification

### Abstraction Layer
**Location**: [`packages/relayer/lib/`](packages/relayer/lib/) - Common interfaces and utilities

The system abstracts blockchain differences through common interfaces:
- **Chain Abstraction**: Generic interface for different blockchain types
- **Event Processing**: Unified event fetching and parsing
- **Transaction Building**: Consistent transaction generation patterns