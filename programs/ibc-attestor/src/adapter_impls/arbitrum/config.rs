use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ArbitrumClientConfig {
    pub url: String,
    pub router_address: String,
}
