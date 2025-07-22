# IBC Attestor
This program has two functionalities. Running an IBC attestor and generating secp256k1 key pairs.

## Development Quick start
1. Run: `cargo run -- key generate` to generate a private key
2. Run: `cargo run -- key show` to show your keys
3. Run `cargo run -- server solana --config server.dev.toml` to start a dev server
4. Query the server by running: `grpcurl \
  -plaintext \
  -d '{"height": 391637727}' \
  localhost:8080 \
  ibc_attestor.AttestationService/GetAttestationsFromHeight | jq`
