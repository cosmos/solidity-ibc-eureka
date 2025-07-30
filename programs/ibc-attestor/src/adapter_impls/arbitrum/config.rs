use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ArbitrumClientConfig {
    pub url: String,
    pub router_address: String,
}
