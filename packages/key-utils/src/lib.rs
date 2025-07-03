use pem::parse as parse_pem;
use pkcs8::PrivateKeyInfo;
use sec1::{der::Decode, EcPrivateKey};
use secp256k1::{PublicKey, SecretKey};
use std::{fs, path::Path};

/// Read a secp256k1 private key from PEM (PKCS#8 or SEC1) and return a `SecretKey`.
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
pub fn read_private_key_pem<P: AsRef<Path>>(path: P) -> Result<SecretKey, anyhow::Error> {
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

    let ec = EcPrivateKey::from_der(&sec1_der).map_err(|e| anyhow::anyhow!("{e}"))?;
    let raw = ec.private_key;
    let arr: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("SEC1 privateKey OCTET STRING must be 32 bytes"))?;

    Ok(SecretKey::from_byte_array(arr).map_err(|e| anyhow::anyhow!("{e}"))?)
}

/// Parse a compressed (33-byte) public key from a byte slice.
///
/// # Errors
/// Returns an Error if the slice is not exactly 33 bytes
/// or not a valid public key encoding.
pub fn parse_public_key(bytes: &[u8]) -> Result<PublicKey, anyhow::Error> {
    PublicKey::from_slice(bytes).map_err(|e| e.into())
}

#[cfg(test)]
mod read_private_key_pem {

    use super::*;
    use pkcs8::{ObjectIdentifier, SecretDocument};
    use sec1::{
        pem::{LineEnding, PemLabel},
        EcParameters, EcPrivateKey,
    };
    use secp256k1::rand::{rng, RngCore};
    use std::{env, fs, path::PathBuf};

    fn create_random_private_key() -> PathBuf {
        let mut rng = rng();
        let mut raw = [0u8; 32];
        rng.fill_bytes(&mut raw);
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
        assert!(read_private_key_pem(&tmp).is_ok());
    }

    #[test]
    fn fails_on_missing_file() {
        let tmp = env::temp_dir().join("not_a_key.pem");
        fs::write(&tmp, "this is not PEM").unwrap();
        assert!(read_private_key_pem(&tmp).is_err());
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
        let err = read_private_key_pem(&tmp);
        assert!(err.is_err());
    }
}

#[cfg(test)]
mod parse_public_key {
    use super::*;
    use secp256k1::rand::rng;
    use secp256k1::Secp256k1;

    #[test]
    fn succeeds_on_valid_pkey() {
        let secp = Secp256k1::new();
        let mut rng = rng();
        let (_, pk) = secp.generate_keypair(&mut rng);

        let comp = pk.serialize();

        let pk2 = parse_public_key(&comp).unwrap();
        assert_eq!(pk, pk2);
    }

    #[test]
    fn fails_on_wrong_size() {
        let secp = Secp256k1::new();
        let mut rng = rng();
        let (_, pk) = secp.generate_keypair(&mut rng);

        let mut comp = pk.serialize().to_vec();
        comp.push(8);

        let failed = parse_public_key(&comp);
        assert!(failed.is_err());
    }
}
