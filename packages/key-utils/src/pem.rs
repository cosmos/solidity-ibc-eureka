#![allow(clippy::module_name_repetitions)]

use alloy_signer_local::PrivateKeySigner;

use pem::parse as parse_pem;
use pkcs8::PrivateKeyInfo;

use sec1::EncodeEcPrivateKey;
use sec1::{der::Decode, pem::LineEnding, EcPrivateKey};

use std::{fs, path::Path};

/// Generate a random secp256k1 private key and write it as SEC1 PEM.
pub fn generate_private_key_pem<P: AsRef<Path>>(path: P) -> Result<(), anyhow::Error> {
    let signer = PrivateKeySigner::random();
    write_sec1_pem(path, &signer)
}

/// Read a secp256k1 private key from PEM (PKCS#8 or SEC1) and return a `PrivateKeySigner`.
pub fn read_private_key_pem<P: AsRef<Path>>(path: P) -> Result<PrivateKeySigner, anyhow::Error> {
    let pem_str = fs::read_to_string(path)?;
    let pem = parse_pem(&pem_str)?;

    let sec1_der = match pem.tag() {
        "PRIVATE KEY" => {
            let pki =
                PrivateKeyInfo::from_der(pem.contents()).map_err(|e| anyhow::anyhow!("{e}"))?;
            pki.private_key
        }
        "EC PRIVATE KEY" => pem.contents(),
        other => return Err(anyhow::anyhow!("unexpected PEM label: {}", other)),
    };

    let ec = EcPrivateKey::from_der(sec1_der).map_err(|e| anyhow::anyhow!("{e}"))?;
    let raw = ec.private_key;
    let arr: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("SEC1 privateKey OCTET STRING must be 32 bytes"))?;

    PrivateKeySigner::from_slice(&arr).map_err(|e| anyhow::anyhow!("{e}"))
}

/// Write a SEC1 (EC PRIVATE KEY) PEM file for the given signer.
fn write_sec1_pem<P: AsRef<Path>>(path: P, signer: &PrivateKeySigner) -> Result<(), anyhow::Error> {
    let pem_bytes = signer
        .credential()
        .to_sec1_pem(LineEnding::default())
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .as_bytes()
        .to_vec();
    fs::write(path, pem_bytes).map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn write_then_read_roundtrip() {
        let tmp = env::temp_dir().join("sec1_roundtrip_key.pem");
        let signer = PrivateKeySigner::random();
        write_sec1_pem(&tmp, &signer).unwrap();
        let loaded = read_private_key_pem(&tmp).unwrap();
        assert_eq!(signer.address(), loaded.address());
    }
}
