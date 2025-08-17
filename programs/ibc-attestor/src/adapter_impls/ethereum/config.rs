#[derive(Clone, Debug, serde::Deserialize)]
pub struct EthClientConfig {
    pub url: String,
    pub router_address: String,
}


