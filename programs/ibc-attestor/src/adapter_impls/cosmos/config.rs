use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct CosmosClientConfig {
    pub url: String,
}
