#![allow(unused_crate_dependencies)]

use ethereum_light_client::{
    client_state::ClientState, consensus_state::ConsensusState, header::Header,
};

#[cfg(feature = "test-utils")]
use ethereum_light_client::test_utils::fixtures::{
    CommitmentProof, InitialState, Step, StepsFixture, UpdateClient,
};

use ethereum_types::execution::storage_proof::StorageProof;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
struct EthereumTypes {
    client_state: ClientState,
    consensus_state: ConsensusState,
    header: Header,
    storage_proof: StorageProof,
    #[cfg(feature = "test-utils")]
    test_fixtures: TestFixtures,
}

#[cfg(feature = "test-utils")]
#[derive(Serialize, Deserialize, JsonSchema)]
struct TestFixtures {
    steps_fixture: StepsFixture,
    step: Step,
    initial_state: InitialState,
    commitment_proof: CommitmentProof,
    update_client: UpdateClient,
}

fn main() {
    let schema = schemars::schema_for!(EthereumTypes);
    std::fs::write(
        "ethereum_types_schema.json",
        serde_json::to_string_pretty(&schema).expect("Failed to serialize schema"),
    )
    .expect("Failed to write schema to file");
}
