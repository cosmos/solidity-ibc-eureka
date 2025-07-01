use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;

#[derive(Debug, Deserialize)]
pub struct SolanaClientConfig {
    pub url: String,
    pub account_key: Pubkey,
}
