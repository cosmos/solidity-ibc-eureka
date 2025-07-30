use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct OpConsensusClientConfig {
    pub url: String,
    pub router_address: String,
}
