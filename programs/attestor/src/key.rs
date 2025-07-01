use anyhow::{Context, Result};
use secp256k1::{generate_keypair, rand, PublicKey, Secp256k1, SecretKey};
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

const KEY_FILE_NAME: &str = "secp256k1";
const CONFIG_DIR_NAME: &str = ".attestor";

#[derive(Error, Debug)]
pub enum KeyError {
    #[error("Home directory not found")]
    HomeDirNotFound,
    #[error("Key file not found at {0}. Please generate a key first using `attestor generate`.")]
    KeyNotFound(String),
}

/// Returns the path to the configuration directory.
fn get_config_dir() -> Result<PathBuf> {
    let home_dir = dirs::home_dir().ok_or(KeyError::HomeDirNotFound)?;
    Ok(home_dir.join(CONFIG_DIR_NAME))
}

/// Returns the path to the key file.
fn get_key_path() -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    Ok(config_dir.join(KEY_FILE_NAME))
}

/// Stores the secret key in the configuration directory.
fn store_secret_key(secret_key: &SecretKey) -> Result<PathBuf> {
    let config_dir = get_config_dir()?;
    fs::create_dir_all(&config_dir)
        .with_context(|| format!("Failed to create config directory at {:?}", config_dir))?;

    let key_path = get_key_path()?;
    fs::write(&key_path, secret_key.secret_bytes())
        .with_context(|| format!("Failed to write key to {:?}", key_path))?;
    Ok(key_path)
}

/// Reads the secret key from the configuration directory.
fn read_secret_key() -> Result<SecretKey> {
    let key_path = get_key_path()?;
    if !Path::new(&key_path).exists() {
        return Err(KeyError::KeyNotFound(key_path.display().to_string()).into());
    }
    let secret_bytes = fs::read(&key_path)
        .with_context(|| format!("Failed to read key from {:?}", key_path))?;
    SecretKey::from_slice(&secret_bytes)
        .with_context(|| "Failed to create secret key from slice")
}

/// Reads the public key from the stored secret key.
fn read_public_key() -> Result<PublicKey> {
    let secret_key = read_secret_key()?;
    let secp = Secp256k1::new();
    Ok(PublicKey::from_secret_key(&secp, &secret_key))
}

/// Generates a new key pair, stores it, and prints the public key.
pub fn generate_and_store_key_pair() -> Result<()> {
    let (secret_key, public_key) = generate_keypair(&mut rand::rng());
    let key_path = store_secret_key(&secret_key)?;
    println!("New key pair generated and stored successfully at {:?}", key_path);
    println!("Public key (uncompressed): {}", public_key);
    Ok(())
}

/// Reads the public key from storage and prints it.
pub fn show_public_key() -> Result<()> {
    let public_key = read_public_key()?;
    println!("Public key (uncompressed): {}", public_key);
    Ok(())
}