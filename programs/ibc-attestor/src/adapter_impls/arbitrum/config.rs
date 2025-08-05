use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ArbitrumClientConfig {
    pub url: String,
    pub router_address: String,
}
