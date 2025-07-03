use std::str::FromStr;

use solana_client::nonblocking::rpc_client::RpcClient;
use solana_commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;

mod account_state;
mod config;

use crate::adapter_client::{Adapter, AdapterError, Signable};
use crate::cli::SolanaClientConfig;

pub use account_state::AccountState;

///
/// Relevant chain peek options. For their Solana
/// interpretation see [these docs](https://docs.arbitrum.io/for-devs/troubleshooting-building#how-many-block-numbers-must-we-wait-for-in-arbitrum-before-we-can-confidently-state-that-the-transaction-has-reached-finality)
enum PeekKind {
    /// Most recent confirmed L2 block on ETH L1
    Finalized,
    /// Latest L2 block
    Latest,
}

pub struct SolanaClient {
    client: RpcClient,
    account_key: Pubkey,
}

impl SolanaClient {
    pub fn from_config(config: SolanaClientConfig) -> Self {
        let client = RpcClient::new(config.url);
        let account_key = Pubkey::from_str(&config.account_key).unwrap();
        Self {
            client,
            account_key,
        }
    }

    async fn get_account_info_by_slot_height(
        &self,
        peek_kind: &PeekKind,
    ) -> Result<AccountState, AdapterError> {
        let commitment = match peek_kind {
            PeekKind::Finalized => CommitmentConfig::finalized(),
            PeekKind::Latest => CommitmentConfig::confirmed(),
        };

        let account_info = self
            .client
            .get_account_with_commitment(&self.account_key, commitment)
            .await
            .map(|r| (r.context.slot, r.value.map(|acc| acc.data)))
            .map_err(|e| AdapterError::FinalizedBlockError(e.to_string()))?;

        match account_info.1 {
            Some(data) => Ok(AccountState {
                slot: account_info.0,
                data,
            }),
            None => Err(AdapterError::FinalizedBlockError(format!(
                "no account found for {}",
                self.account_key
            ))),
        }
    }
}

impl Adapter for SolanaClient {
    async fn get_latest_finalized_block(&self) -> Result<impl Signable, AdapterError> {
        let account_state = self
            .get_account_info_by_slot_height(&PeekKind::Finalized)
            .await?;

        Ok(account_state)
    }
    async fn get_latest_unfinalized_block(&self) -> Result<impl Signable, AdapterError> {
        let account_state = self
            .get_account_info_by_slot_height(&PeekKind::Latest)
            .await?;

        Ok(account_state)
    }
    #[inline]
    fn block_time_ms(&self) -> u64 {
        400
    }
}
