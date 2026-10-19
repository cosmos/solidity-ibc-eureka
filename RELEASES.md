# Releases

This repository has multiple on-chain and off-chain components. Each component has its own release process.

## On-chain components

### Solidity Contracts

The solidity contracts releases are tagged with the `solidity` prefix. For example, `solidity-v2.0.1`. Solidity contract releases follow semantic versioning.

- **Major version bump**: A major version bump indicates that there are breaking changes in the API (regardless of whether they require a new initializer or not).
- **Minor version bump**: A minor version bump indicates that there are non-breaking changes in the API that require a new initializer (due to storage layout changes).
- **Patch version bump**: A patch version bump indicates that there are non-breaking changes in the API that do not require a new initializer (no storage layout changes).

### CosmWasm Ethereum Light Client

The CosmWasm Ethereum Light Client releases are tagged with the `cw-ics08-wasm-eth` prefix. For example, `cw-ics08-wasm-eth-v1.3.0`. CosmWasm Ethereum Light Client releases follow semantic versioning.

- **Major version bump**: A major version bump indicates that there are breaking changes in the API. (This can only happen if there is an API breaking change in `ibc-go`'s `08-wasm` module, since the API is defined by the `ibc-go` interface.)
- **Minor version bump**: A minor version bump indicates that there are non-breaking changes in the API that require a state migration (`migrate` entry point) due to storage layout changes.
- **Patch version bump**: A patch version bump indicates that there are non-breaking changes in the API that do not require a state migration (no storage layout changes).

### Solana Programs

The Solana programs releases are tagged with the `solana` prefix. For example, `solana-v1.0.0`. Solana program releases follow semantic versioning.

- **Major version bump**: A major version bump indicates that there are breaking changes in the API.
- **Minor version bump**: A minor version bump indicates that there are non-breaking changes in the API that require storage layout changes.
- **Patch version bump**: A patch version bump indicates that there are non-breaking changes in the API that do not require storage layout changes.

## Off-chain components

### Proof API

The proof API releases are tagged with the `proof-api` prefix. For example, `proof-api-v0.7.0`. Proof API releases follow semantic versioning.

The proof API does not have a major release yet, since we want to reserve the right to make breaking changes to the proof API until we have a stable API that we are confident will not require breaking changes in the future.

- **Minor version bump**: A minor version bump indicates that there are new features or improvements in the proof API.
- **Patch version bump**: A patch version bump indicates that there are bug fixes or minor improvements in the proof API.

## Signed release assets

Pushing a `solidity-v*`, `sp1-programs-v*`, `cw-ics08-wasm-eth-v*` or `proof-api-v*` tag runs the [release workflow](./.github/workflows/release.yml). It builds the release assets of the component from the tagged commit, generates an SPDX SBOM from the component's lockfiles, attests their build provenance, signs a `checksums.txt` over them with cosign, and attaches everything to a draft release for the tag. The release notes get a section explaining how to verify the assets.

| Asset | Purpose |
| --- | --- |
| Component assets (`solidity-contracts-<tag>.tar.gz`, the `sp1-ics07-tendermint-*` programs, `cw_ics08_wasm_eth.wasm.gz`, or `<tag>-linux-amd64.tar.gz` and `<tag>-linux-arm64.tar.gz` with the `proof-api` and `operator` binaries) | What the component ships |
| `<tag>.spdx.json` (`<tag>-<platform>.spdx.json` for proof-api) | SPDX SBOM |
| `checksums.txt` | SHA-256 of the assets and the SBOM |
| `checksums.txt.sigstore.json` | Cosign signature over `checksums.txt` |
| `<tag>.sigstore.jsonl` | Provenance and SBOM attestation bundles, for `gh attestation verify --bundle` without the attestations API |

The same release workflow runs in [cosmos/ibc](https://github.com/cosmos/ibc) and [cosmos/ibc-go](https://github.com/cosmos/ibc-go); only the tag patterns and the build differ.

### Release procedure

1. Tag the release commit and push the tag (e.g. `git tag -s solidity-v3.0.3 -m solidity-v3.0.3 origin/main && git push origin solidity-v3.0.3`).
2. Wait for the [release workflow](https://github.com/cosmos/ibc-contracts/actions/workflows/release.yml) to create the draft release in [GitHub](https://github.com/cosmos/ibc-contracts/releases) with the signed assets attached. The draft is only created once every build job has passed. If one fails, re-run the failed jobs.
3. Edit the draft and write the release notes above the generated "Verifying the release" section.
4. Check the `Set as a pre-release` checkbox if needed. The workflow already checks it for tags with a pre-release suffix such as `-rc.1`.
5. Publish the release.

Push the tag with `git` before publishing the release. A release drafted from the GitHub UI only creates its tag when it is published, so its assets would be attached after publishing. The workflow runs from the tagged commit, so a release branch must contain the current `.github/workflows/release.yml`.

### Container images

A `proof-api-v*` tag also runs the [docker workflow](./.github/workflows/docker.yml), which publishes `ghcr.io/cosmos/proof-api:vX.Y.Z`. The workflow attests the build provenance and per-platform SBOMs of every `ghcr.io/cosmos/proof-api` image it publishes, and signs it with cosign. Images built for pull requests are not signed. Verify an image with:

```sh
cosign verify ghcr.io/cosmos/proof-api:<tag> \
  --certificate-identity-regexp '^https://github.com/cosmos/ibc-contracts/.github/workflows/docker.yml@' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
gh attestation verify oci://ghcr.io/cosmos/proof-api:<tag> --repo cosmos/ibc-contracts \
  --signer-workflow cosmos/ibc-contracts/.github/workflows/docker.yml
```

Image SBOMs are attached to the platform digests, not the multi-platform tag. To check one, pass the platform digest with `--predicate-type https://spdx.dev/Document/v2.3`.
