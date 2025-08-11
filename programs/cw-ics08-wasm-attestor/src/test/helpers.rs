//! Test helpers for Solana light client tests

use std::{env, fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

use attestor_light_client::test_utils::compressed_pubkeys_blob;
use cosmwasm_std::testing::{mock_dependencies, MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{Empty, OwnedDeps};

pub fn mk_deps() -> OwnedDeps<MockStorage, MockApi, MockQuerier, Empty> {
    mock_dependencies()
}

fn tmp_file_path(prefix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    p.push(format!("{}-{}-{}.bin", prefix, std::process::id(), nanos));
    p
}

pub fn write_compressed_pubkeys_and_set_env() -> PathBuf {
    let path = tmp_file_path("attestor-pubkeys");
    let buf = compressed_pubkeys_blob();
    fs::write(&path, buf).unwrap();
    env::set_var("PUB_KEYS_PATH", &path);
    path
}

pub fn setup_pubkeys_env() -> PathBuf {
    write_compressed_pubkeys_and_set_env()
}
