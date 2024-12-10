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
