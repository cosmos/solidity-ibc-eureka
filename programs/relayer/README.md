# IBC Proof API

The proof API is a gRPC service that prepares IBC transactions and proofs for clients. It does not listen for events, sign, or submit transactions. Instead, clients provide transaction hashes and the service returns the corresponding IBC messages and proofs that must be signed and submitted elsewhere.

The proof API:
1. Queries the source chain for the given transaction hashes.
2. Parses transaction events to extract packet data.
3. Builds IBC messages plus the required proof(s).
4. Returns an unsigned transaction payload to the caller.

## Overview

The service is composed of one-sided modules, each responsible for a specific source and target chain combination. Each module is a Rust struct that implements the [`RelayerModule`](https://github.com/cosmos/solidity-ibc-eureka/blob/debc0ad73acab0cd0a827a1a35a7ae4c1c65feb1/relayer/src/core/modules.rs#L10) trait.

The gRPC service definition is in [`relayer/proto/relayer/relayer.proto`](https://github.com/cosmos/solidity-ibc-eureka/blob/debc0ad73acab0cd0a827a1a35a7ae4c1c65feb1/relayer/proto/relayer/relayer.proto).

## Modules

Each module runs in one direction and specializes in how it builds proofs and transactions:

- `cosmos_to_eth`: Extracts IBC packet events from a Cosmos SDK chain, generates proofs (SP1 or attested), and builds transactions for an EVM-based chain.
- `eth_to_cosmos`: Extracts packet events from an EVM chain, prepares IBC messages for a Cosmos SDK chain, and packages the required proof data (real, mock, or attested).
- `eth_to_eth`: Extracts packet events from an EVM chain and builds attested transactions for another EVM chain.
- `cosmos_to_cosmos`: Extracts packet events from a Cosmos SDK chain and prepares IBC messages for another Cosmos SDK chain.
- `cosmos_to_solana`: Extracts packet events from a Cosmos SDK chain and prepares IBC messages for Solana.
- `solana_to_cosmos`: Extracts packet events from Solana and prepares IBC messages for a Cosmos SDK chain.

## Build and Run

1. Build/install the binary.

   ```sh
   just install-relayer
   ```

2. Copy the example configuration and update it for your chains.

   ```sh
   cp programs/relayer/config.example.json config.json
   ```

3. Start the service.

   ```sh
   relayer -c config.json
   ```

The gRPC server listens on `server.address:server.port` and the gRPC-web endpoint listens on `server.grpc_web_port`.

## Configuration

Configuration is provided as JSON. See the example in [`programs/relayer/config.example.json`](./config.example.json).

Key settings:

- `server.address`: IP address to bind the gRPC server.
- `server.port`: gRPC port.
- `server.grpc_web_port`: gRPC-web port.
- `observability.level`: Logging level (`trace`, `debug`, `info`, `warn`, `error`).
- `observability.use_otel`: Enable OpenTelemetry export.
- `observability.service_name`: Service name used in logs/traces.
- `observability.otel_endpoint`: OpenTelemetry collector endpoint.
- `modules`: List of modules to run, each with `name`, `src_chain`, `dst_chain`, and module-specific `config`.

Module configuration varies by module type:

- `cosmos_to_eth`:
  - `tm_rpc_url`: Tendermint RPC endpoint for the source chain.
  - `eth_rpc_url`: EVM RPC endpoint for the destination chain.
  - `ics26_address`: IBC router contract address on the destination chain.
  - `mode`: Proof mode (`sp1` or `attested`) and its settings.
- `eth_to_cosmos`:
  - `eth_rpc_url`: EVM RPC endpoint for the source chain.
  - `eth_beacon_api_url`: Ethereum beacon API endpoint for consensus data (required for `real`).
  - `tm_rpc_url`: Tendermint RPC endpoint for the destination chain.
  - `ics26_address`: IBC router contract address on the destination chain.
  - `signer_address`: Cosmos address used for message construction metadata.
  - `mode`: Proof mode (`real`, `mock`, or `attested`).
- `eth_to_cosmos_compat`:
  - Same settings as `eth_to_cosmos`, but uses both the current and v1.2 handlers internally.
- `eth_to_eth`:
  - `src_chain_id`: Source chain ID string.
  - `src_rpc_url`: Source EVM RPC endpoint.
  - `src_ics26_address`: Source IBC router contract address.
  - `dst_rpc_url`: Destination EVM RPC endpoint.
  - `dst_ics26_address`: Destination IBC router contract address.
  - `mode`: Attestation settings (`attested`).
- `cosmos_to_cosmos`:
  - `src_rpc_url`: RPC endpoint for the source chain.
  - `target_rpc_url`: RPC endpoint for the destination chain.
  - `signer_address`: Cosmos address used for message construction metadata.
- `cosmos_to_solana`:
  - `source_rpc_url`: Tendermint RPC endpoint for the source chain.
  - `target_rpc_url`: Solana RPC endpoint.
  - `solana_ics26_program_id`: Solana ICS26 router program ID.
  - `solana_fee_payer`: Solana fee payer address.
  - `solana_alt_address`: Address lookup table address (optional).
  - `mock_wasm_client`: Enable mock Cosmos WASM client mode.
  - `skip_pre_verify_threshold`: Skip pre-verification when signatures are below this threshold. (default: 50)
- `solana_to_cosmos`:
  - `solana_chain_id`: Solana chain ID label.
  - `src_rpc_url`: Solana RPC endpoint.
  - `target_rpc_url`: Tendermint RPC endpoint for the destination chain.
  - `signer_address`: Cosmos address used for message construction metadata.
  - `solana_ics26_program_id`: Solana ICS26 router program ID.
  - `mock_wasm_client`: Enable mock Cosmos WASM client mode.
  - `mock_solana_client`: Enable mock Solana client mode.

## Using the gRPC API

After the service is running, use the gRPC endpoints defined in [`relayer/proto/relayer/relayer.proto`](https://github.com/cosmos/solidity-ibc-eureka/blob/debc0ad73acab0cd0a827a1a35a7ae4c1c65feb1/relayer/proto/relayer/relayer.proto) to submit transaction hashes and retrieve the unsigned IBC transactions plus proofs. Typical usage is:

1. Submit transaction hash(es) to the proof API for the relevant module.
2. Receive the unsigned transaction payload with proof data.
3. Sign and submit the transaction using your own wallet or service.
