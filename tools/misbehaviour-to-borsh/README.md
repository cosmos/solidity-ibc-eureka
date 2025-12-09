# misbehaviour-to-borsh

A CLI tool that converts Tendermint misbehaviour data from Protobuf format to Borsh format for use with Solana IBC programs.

## Purpose

Solana programs use Borsh serialization while IBC/Cosmos chains use Protobuf. This tool bridges that gap by:

1. Reading Protobuf-encoded Tendermint misbehaviour from stdin
2. Converting it to the Borsh format expected by Solana IBC light client
3. Outputting the hex-encoded Borsh bytes to stdout

## Usage

```bash
# Pipe protobuf bytes and get hex-encoded Borsh output
cat misbehaviour.pb | cargo run --release --quiet
```

## Integration

This tool is called by the Go e2e test code in `e2e/interchaintestv8/solana/borsh_misbehaviour.go` to prepare misbehaviour data for submission to the Solana ICS07 Tendermint light client.
