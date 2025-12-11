# Operator CLI (SP1 fixture generator)

`operator` produces SP1 ICS07 Tendermint fixtures used by Solidity, Rust, and Go tests.

## Install
- `just install-operator` (or `cargo install --bin operator --path programs/operator --locked`).
- Requires SP1 toolchain (`~/.sp1/bin/cargo-prove`) and the ELF binaries from `just build-sp1-programs` (defaults baked into flags).

## Environment
- Copy `.env.example` → `.env`; set `TENDERMINT_RPC_URL` for the source chain.
- For network proving, set `SP1_PROVER=network` and `NETWORK_PRIVATE_KEY=<hex>` (optional `E2E_PRIVATE_CLUSTER=true`). `SP1_PROVER=mock` skips network calls.

## Common commands
- Update client proof:
  ```sh
  operator fixtures update-client \
    --trusted-block 1000 --target-block 1010 \
    --proof-type plonk \
    -o test/sp1-ics07/fixtures/update_client_fixture-plonk.json
  ```
- Membership proof (comma-separated key paths):
  ```sh
  operator fixtures membership \
    --trusted-block 1000 \
    --key-paths clients/07-tendermint-0/clientState,ibc/commitments/ports/transfer/channels/channel-0/packets/1 \
    --proof-type plonk \
    -o test/sp1-ics07/fixtures/memberships_fixture-plonk.json
  ```
- Update + membership in one shot:
  ```sh
  operator fixtures update-client-and-membership \
    --trusted-block 1000 --target-block 1010 \
    --key-paths clients/07-tendermint-0/clientState \
    --proof-type plonk \
    -o test/sp1-ics07/fixtures/uc_and_memberships_fixture-plonk.json
  ```
- Misbehaviour proof (input JSON conforms to `IICS07TendermintMisbehaviour`):
  ```sh
  operator fixtures misbehaviour \
    --misbehaviour-json-path ./misbehaviour.json \
    --proof-type groth16 \
    -o test/sp1-ics07/fixtures/misbehaviour_fixture-groth16.json
  ```

Flags automatically point to the compiled ELF binaries; override with `--update-client-path` etc. Use `--trust-level`, `--trusting-period`, and `--private-cluster` to tune security parameters and prover backend.
