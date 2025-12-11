# POC Relayer Implementation

This is a proof of concept implementation of a relayer server for `solidity-ibc-eureka` and Cosmos SDK based chains.

This relayer works differently from other relayers in that it neither listens to events nor submits transactions to any chain. Instead, it runs a gRPC server that can be queried by a client to get the transactions that need to be submitted to the chain to relay packets.

The client submits the hashes of the transactions that need to be relayed to the relayer, and the relayer:
1. Queries the chain for the transactions with the given hashes.
2. Parses the transaction events to get the packet data.
3. Generates the corresponding IBC transactions and proof into a single transaction.
4. Does not sign nor submit the transaction to the chain, but returns it to the client.

In essence, this relayer is meant to be used in a setup where the client is a front-end application, or a service that can sign and submit transactions to the chain.

## Overview

The relayer is composed of multiple one-sided relayer servers, each of which is responsible for relaying packets from one chain to another. A relayer module is a rust struct that implements the [`RelayerModule`](https://github.com/cosmos/solidity-ibc-eureka/blob/debc0ad73acab0cd0a827a1a35a7ae4c1c65feb1/relayer/src/core/modules.rs#L10) trait.

You can see the protocol buffer definition for the gRPC service [here](https://github.com/cosmos/solidity-ibc-eureka/blob/debc0ad73acab0cd0a827a1a35a7ae4c1c65feb1/relayer/proto/relayer/relayer.proto).

This is a work-in-progress implementation, and the relayer is not yet usable. The relayer will only be able to relay IBC Eureka packets. There is a tracking issue for the relayer [here](https://github.com/cosmos/solidity-ibc-eureka/issues/121).

| **Source Chain** | **Target Chain** | **Light Client** | **Development Status** |
|:---:|:---:|:---:|:---:|
| Cosmos SDK | EVM | `sp1-ics07-tendermint` | ✅ |
| EVM | Cosmos SDK | `cw-ics08-wasm-eth` | ✅ |
| Cosmos SDK | Cosmos SDK | `07-tendermint` | ✅ |

## Quickstart

1) Install: `just install-relayer` (or `cargo install --bin relayer --path programs/relayer --locked`).
2) Copy and edit `config.example.json` -> `config.json`. At minimum set:
   - `tm_rpc_url`, `eth_rpc_url`, `eth_beacon_api_url` (when eth->cosmos),
   - `ics26_address` (deployed router), `signer_address` (cosmos account for cosmos->* paths),
   - SP1 prover block: choose `network` with `network_private_key` (see `.env.example`) or `mock` for local/dev.
   - Paths to SP1 ELF binaries if you built them elsewhere.
3) Run: `relayer -c config.json`. Use `RUST_LOG=info` and `ENABLE_LOCAL_OBSERVABILITY=true` to emit OTLP locally (see `.env.example`).

### Minimal cosmos->eth config snippet
```json
{
  "server": { "address": "127.0.0.1", "port": 3000 },
  "modules": [
    {
      "name": "cosmos_to_eth",
      "src_chain": "cosmoshub-4",
      "dst_chain": "0x1",
      "config": {
        "tm_rpc_url": "http://localhost:26657",
        "ics26_address": "0xYourRouter",
        "eth_rpc_url": "http://localhost:8545",
        "sp1_prover": { "type": "mock" },
        "sp1_programs": {
          "update_client": "programs/sp1-programs/.../sp1-ics07-tendermint-update-client",
          "membership": "programs/sp1-programs/.../sp1-ics07-tendermint-membership",
          "update_client_and_membership": "programs/sp1-programs/.../sp1-ics07-tendermint-uc-and-membership",
          "misbehaviour": "programs/sp1-programs/.../sp1-ics07-tendermint-misbehaviour"
        }
      }
    }
  ]
}
```

### Run
```sh
RUST_LOG=info relayer -c config.json
```

Use `--help` to see command-line options and available flags.
