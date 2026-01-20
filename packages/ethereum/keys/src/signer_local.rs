use alloy_signer_local::{LocalSigner, PrivateKeySigner};
use rand::thread_rng;

use std::path::Path;

/// Read a secp256k1 private key from keystore and return a `PrivateKeySigner`.
pub fn read_from_keystore<P: AsRef<Path>>(path: P) -> Result<PrivateKeySigner, anyhow::Error> {
    let signer = LocalSigner::decrypt_keystore(path, "")?;
    Ok(signer)
}

/// Write a secp256k1 private key to keystore.
pub fn write_to_keystore<P: AsRef<Path>>(
    folder_path: P,
    name: &str,
    signer: PrivateKeySigner,
) -> Result<(), anyhow::Error> {
    let key = signer.credential().to_bytes();

    let mut rng = thread_rng();
    let (_, id) = LocalSigner::encrypt_keystore(folder_path, &mut rng, key, "", Some(name))?;
    println!("id: {id}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_signer_local::PrivateKeySigner;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn write_then_read_roundtrip() {
        let name = "sec1_roundtrip_key";
        let tmp_dir = tempdir().unwrap();
        let signer = PrivateKeySigner::random();

        write_to_keystore(tmp_dir.path(), name, signer.clone()).unwrap();

        fs::read_dir(tmp_dir.path()).unwrap().for_each(|entry| {
            println!("entry: {:?}", entry.unwrap().path());
        });

        let keystore_path = tmp_dir.path().join(name);
        let loaded = read_from_keystore(&keystore_path).unwrap();
        assert_eq!(signer.address(), loaded.address());
    }
}
