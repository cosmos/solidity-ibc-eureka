use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct OpClientConfig {
    pub url: String,
    pub router_address: String,
}
