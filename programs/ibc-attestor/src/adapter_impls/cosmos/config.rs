#[derive(Clone, Debug, serde::Deserialize)]
pub struct CosmosClientConfig {
    pub url: String,
    #[serde(default = "CosmosClientConfig::default_store_prefix")]
    pub store_prefix: String,
}

impl CosmosClientConfig {
    fn default_store_prefix() -> String { "ibc".to_string() }
}


