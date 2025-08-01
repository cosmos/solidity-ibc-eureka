use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct OpClientConfig {
    pub url: String,
    pub router_address: String,
}
