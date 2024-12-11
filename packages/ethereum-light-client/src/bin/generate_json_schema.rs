use ethereum_light_client::{
    client_state::ClientState,
    consensus_state::ConsensusState,
    types::{light_client::Header, storage_proof::StorageProof},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema)]
struct EthereumTypes {
    client_state: ClientState,
    consensus_state: ConsensusState,
    header: Header,
    storage_proof: StorageProof,
}

fn main() {
    let schema = schemars::schema_for!(EthereumTypes);
    std::fs::write(
        "ethereum_types_schema.json",
        serde_json::to_string_pretty(&schema).expect("Failed to serialize schema"),
    )
    .expect("Failed to write schema to file");
}
