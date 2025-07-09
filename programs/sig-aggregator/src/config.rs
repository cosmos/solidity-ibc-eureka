use anyhow::Result;
use serde::{Deserialize};
use url::Url;
use std::{fs, net::SocketAddr, path::Path};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub attestor_endpoints: Vec<Url>,
    pub quorum_threshold: usize,
    pub listen_addr: SocketAddr,
    pub attestor_query_timeout_ms: u64,
}

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}
