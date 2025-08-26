key-utils
=========

Small helpers for SHA-256-prehashed ECDSA on secp256k1 built on alloy.

- Uses alloy-primitives (Signature, B256, Address) and alloy-signer::SignerSync.
- PEM I/O and key generation are feature-gated to avoid RNG/FS in wasm.

Features
--------
- std (default)
- pem-io: enable PEM read/write via pem, pkcs8, sec1
- local-signer: enable alloy-signer-local::PrivateKeySigner
- keygen: depends on pem-io + local-signer; writes SEC1 PEM files

API
---
- pem (feature pem-io):
  - read_private_key_signer(path) -> PrivateKeySigner
  - write_sec1_pem(path, &PrivateKeySigner)
- keygen (features keygen + local-signer):
  - generate_private_key_pem(path)
- sign:
  - sign_prehash(&impl SignerSync, &B256) -> Signature
  - sign_sha256(&impl SignerSync, &[u8]) -> Signature
- recover:
  - recover_address_from_prehash(&B256, &[u8]) -> Address
  - recover_address_sha256(&[u8], &[u8]) -> Address
- verify:
  - verify_signature_prehash(Address, &B256, &[u8]) -> bool
  - verify_signature_sha256(Address, &[u8], &[u8]) -> bool

Signatures are 65 bytes (r||s||v). v in {27,28,0,1}.

