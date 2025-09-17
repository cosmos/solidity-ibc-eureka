# IBC Attestor
This program has two functionalities. Running an IBC attestor and generating secp256k1 key pairs.

## Development Quick start
1. Run: `cargo run -- key generate` to generate a private key
2. Run: `cargo run -- key show` to show your keys
3. Run `cargo run -- server op --config server.dev.toml` to start a dev server
4. Query the server by running: `grpcurl \
  -plaintext \
  -d '{"height": 391637727}' \
  localhost:8080 \
  ibc_attestor.AttestationService/GetAttestationsFromHeight | jq`

## Testing
- Run  `just test-e2e TestWithIbcAttestorTestSuite/Test_OptimismAttestToICS20PacketsOnEth` to test EVM -> Cosmos
- Run  `just test-e2e TestCosmosToEVMAttestor` to test Cosmos -> EVM

