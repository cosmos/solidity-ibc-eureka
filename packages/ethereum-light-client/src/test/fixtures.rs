use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    types::{height::Height, storage_proof::StorageProof},
};

#[derive(Serialize, Deserialize, PartialEq, Clone, Debug)]
pub struct CommitmentProofFixture {
    #[serde(with = "ethereum_utils::base64")]
    pub path: Vec<u8>,
    pub storage_proof: StorageProof,
    pub proof_height: Height,
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
}

pub fn load_fixture<T>(name: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    // Construct the path relative to the Cargo manifest directory
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/test");
    path.push(format!("{}.json", name));

    // Open the file and deserialize its contents
    let file = std::fs::File::open(path).unwrap();
    serde_json::from_reader(file).unwrap()
}
