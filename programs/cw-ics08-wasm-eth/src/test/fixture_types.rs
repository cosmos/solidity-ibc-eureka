use ethereum_light_client::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    types::{height::Height, light_client::Header, storage_proof::StorageProof},
};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as};

// TODO: Remove this file once these types are in a separate package #143

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub enum DataType {
    InitialState(Box<InitialState>),
    CommitmentProof(Box<CommitmentProof>),
    UpdateClient(Box<UpdateClient>),
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct InitialState {
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
}

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct CommitmentProof {
    #[serde_as(as = "Base64")]
    pub path: Vec<u8>,
    pub storage_proof: StorageProof,
    pub proof_height: Height,
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Debug)]
pub struct UpdateClient {
    pub client_state: ClientState,
    pub consensus_state: ConsensusState,
    pub updates: Vec<Header>,
}
