//! Test fixtures types and ulitiies for the Ethereum light client

use std::path::PathBuf;

use ibc_proto_eureka::cosmos::tx::v1beta1::TxBody;
use prost::Message;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{client_state::ClientState, consensus_state::ConsensusState};

/// A test fixture with an ordered list of light client operations from the e2e test
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug)]
pub struct StepsFixture {
    /// steps is a list of light client operations
    pub steps: Vec<Step>,
}

/// Step is a light client operation such as an initial state, commitment proof, or update client
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug)]
pub struct Step {
    /// name is the name of the operation, only used for documentation and easy of reading
    pub name: String,
    /// data is the operation data as a JSON object to be deserialized into the appropriate type
    pub data: Value,
}

/// The initial state of the light client in the e2e tests
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug)]
pub struct InitialState {
    /// The client state at the initial state
    pub client_state: ClientState,
    /// The consensus state at the initial state
    pub consensus_state: ConsensusState,
}

/// Operation to update the light client
#[derive(Serialize, Deserialize, JsonSchema, PartialEq, Eq, Clone, Debug)]
pub struct RelayerMessages {
    /// The headers used to update the light client, in order, as a `TxBody`, encoded as hex
    pub relayer_tx_body: String,
}

/// Decode the updates into a `TxBody`
/// # Panics
/// Panics if the updates cannot be decoded into a `TxBody`
#[must_use]
pub fn get_updates_tx_body(updates: String) -> TxBody {
    let tx_body_bz = hex::decode(updates).unwrap();
    TxBody::decode(tx_body_bz.as_slice()).unwrap()
}

impl StepsFixture {
    /// Deserializes the data at the given step into the given type
    /// # Panics
    /// Panics if the data cannot be deserialized into the given type
    #[must_use]
    pub fn get_data_at_step<T>(&self, step: usize) -> T
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_value(self.steps[step].data.clone()).unwrap()
    }
}

/// load loads a test fixture from a JSON file
/// # Panics
/// Panics if the file cannot be opened or the contents cannot be deserialized
#[must_use]
pub fn load<T>(name: &str) -> T
where
    T: serde::de::DeserializeOwned,
{
    // Construct the path relative to the Cargo manifest directory
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/test_utils/fixtures");
    path.push(format!("{name}.json"));

    // Open the file and deserialize its contents
    let file = std::fs::File::open(path).unwrap();
    serde_json::from_reader(file).unwrap()
}
