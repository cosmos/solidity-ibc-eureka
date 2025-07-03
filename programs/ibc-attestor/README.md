# IBC Attestor
This program has two functionalities. Running an IBC attestor and generating secp256k1 key pairs.

## Quick start
1. Run: `sh key-gen.sh` to generate a binary private key
2. Run `cargo run -- server solana --config server.dev.toml` to start a dev server
3. Query the server by running: `grpcurl \
  -plaintext \
  -d '{"height": 391637727}' \
  localhost:8080 \
  ibc_attestor.AttestationService/GetAttestationsFromHeight | jq`
