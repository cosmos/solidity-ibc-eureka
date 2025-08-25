use alloy_signer_local::PrivateKeySigner;
use pem::parse as parse_pem;
use pkcs8::PrivateKeyInfo;
use std::{fs, path::Path};

use pkcs8::{ObjectIdentifier, SecretDocument};
use sec1::{
    der::Decode,
    pem::{LineEnding, PemLabel},
    EcParameters, EcPrivateKey,
};

/// Read a secp256k1 private key from PEM (PKCS#8 or SEC1) and return a `PrivateKeySigner`.
///
/// Supports:
///  - `-----BEGIN PRIVATE KEY-----` (PKCS#8)
///  - `-----BEGIN EC PRIVATE KEY-----` (SEC1)
///
/// # Errors
/// - I/O errors when reading the file
/// - PEM parsing errors if the file isn’t valid PEM
/// - ASN.1 decoding errors if the DER is malformed or the wrong type
/// - Key-length validation if the inner scalar isn’t exactly 32 bytes
/// - `secp256k1` range errors if the scalar is invalid
pub fn read_private_pem_to_secret<P: AsRef<Path>>(
    path: P,
) -> Result<PrivateKeySigner, anyhow::Error> {
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

pub fn generate_secret_key<P: AsRef<Path>>(path: &P) -> Result<(), anyhow::Error> {
    let signer = PrivateKeySigner::random();
    let secret_bytes = signer.to_field_bytes();

    let ec_priv = EcPrivateKey {
        private_key: secret_bytes.as_slice(),
        parameters: Some(EcParameters::NamedCurve(
            ObjectIdentifier::new("1.3.132.0.10").unwrap(),
        )),
        public_key: None,
    };

    let secret_doc = SecretDocument::encode_msg(&ec_priv).expect("DER encoding failed");
    let pem_str = secret_doc
        .to_pem(EcPrivateKey::PEM_LABEL, LineEnding::default())
        .expect("PEM encoding failed");

    fs::write(path, pem_str.as_str()).expect("Failed to write PEM file");
    Ok(())
}

/// Read a secp256k1 private key from PEM (PKCS#8 or SEC1) and return a String.
pub fn read_private_pem_to_string<P: AsRef<Path>>(path: P) -> Result<String, anyhow::Error> {
    Ok(fs::read_to_string(path)?)
}

/// Read a secp256k1 private key from PEM (PKCS#8 or SEC1) and return a String.
pub fn read_public_key_to_string<P: AsRef<Path>>(path: P) -> Result<String, anyhow::Error> {
    let signer = read_private_pem_to_secret(&path)?;
    let address = signer.address();
    Ok(hex::encode(address.as_slice()))
}

/// Parse a compressed (33-byte) public key from a byte slice.
///
/// # Errors
/// Returns an Error if the slice is not exactly 33 bytes
/// or not a valid public key encoding.
pub fn parse_public_key(bytes: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
    if bytes.len() != 33 {
        return Err(anyhow::anyhow!(
            "Public key must be exactly 33 bytes, got {}",
            bytes.len()
        ));
    }
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod generate_and_read_keys {
    use std::collections::HashSet;
    use std::env;

    use super::*;

    #[test]
    fn succeeds() {
        let tmp = env::temp_dir().join("sec1_random_key.pem");
        generate_secret_key(&tmp).unwrap();
        let key = read_private_pem_to_secret(&tmp);
        assert!(key.is_ok());
        let key = read_private_pem_to_string(&tmp);
        assert!(key.is_ok());
        let key = read_public_key_to_string(&tmp);
        assert!(key.is_ok());
    }

    #[test]
    fn is_random() {
        let mut tmps = Vec::new();
        for i in 0..3 {
            let tmp = env::temp_dir().join(format!("sec1_random_key_{i}.pem"));
            tmps.push(tmp);
        }

        let keys: HashSet<String> = tmps
            .iter()
            .map(|t| {
                generate_secret_key(t).unwrap();
                read_private_pem_to_string(t).unwrap()
            })
            .collect();

        assert_eq!(keys.len(), 3);
    }
}

#[cfg(test)]
mod read_private_key_pem {

    use super::*;
    use pkcs8::{ObjectIdentifier, SecretDocument};
    use rand_core::{OsRng, RngCore};
    use sec1::{
        pem::{LineEnding, PemLabel},
        EcParameters, EcPrivateKey,
    };
    use std::{env, fs, path::PathBuf};

    fn create_random_private_key() -> PathBuf {
        let mut raw = [0u8; 32];
        OsRng.fill_bytes(&mut raw);
        let ec_priv = EcPrivateKey {
            private_key: &raw,
            parameters: Some(EcParameters::NamedCurve(
                ObjectIdentifier::new("1.3.132.0.10").unwrap(),
            )),
            public_key: None,
        };

        let secret_doc = SecretDocument::encode_msg(&ec_priv).expect("DER encoding failed");
        let pem_str = secret_doc
            .to_pem(EcPrivateKey::PEM_LABEL, LineEnding::default())
            .expect("PEM encoding failed");

        let tmp = env::temp_dir().join("sec1_random_key.pem");
        fs::write(&tmp, pem_str.as_str()).expect("Failed to write PEM file");

        tmp
    }

    #[test]
    fn succeeds_on_valid_key() {
        let tmp = create_random_private_key();
        // prove that read_private_key_pem successfully parses a valid key
        assert!(read_private_pem_to_secret(&tmp).is_ok());
    }

    #[test]
    fn fails_on_missing_file() {
        let tmp = env::temp_dir().join("not_a_key.pem");
        fs::write(&tmp, "this is not PEM").unwrap();
        assert!(read_private_pem_to_secret(&tmp).is_err());
    }

    #[test]
    fn fails_on_bad_pem() {
        // valid PEM wrapping a too-short OCTET STRING
        let bad_pem = "\
-----BEGIN EC PRIVATE KEY-----
MFECAQAwBQYDK2VwBCIEIw==
-----END EC PRIVATE KEY-----";
        let tmp = env::temp_dir().join("short_key.pem");
        fs::write(&tmp, bad_pem).unwrap();
        let err = read_private_pem_to_secret(&tmp);
        assert!(err.is_err());
    }
}

#[cfg(test)]
mod parse_public_key {
    use super::*;

    #[test]
    fn succeeds_on_valid_pkey() {
        let mut comp = [2u8; 33];
        comp[0] = 0x02;
        for (i, item) in comp.iter_mut().enumerate().skip(1) {
            *item = i as u8;
        }

        let pk2 = parse_public_key(&comp).unwrap();
        assert_eq!(comp.to_vec(), pk2);
    }

    #[test]
    fn fails_on_wrong_size() {
        let mut comp = vec![0x02; 34];
        comp.push(8);

        let failed = parse_public_key(&comp);
        assert!(failed.is_err());
    }
}
