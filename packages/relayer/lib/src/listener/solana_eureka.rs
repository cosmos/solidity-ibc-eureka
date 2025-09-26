//! Solana chain listener implementation for IBC Eureka.

use anyhow::Result;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::UiTransactionEncoding;
use std::sync::Arc;

use crate::{
    chain::SolanaEureka,
    events::solana::{parse_events_from_logs, SolanaEurekaEventWithHeight},
    listener::ChainListenerService,
};

/// The `ChainListener` listens for events on the Solana chain.
pub struct ChainListener {
    rpc_client: Arc<RpcClient>,
    ics26_router_program_id: Pubkey,
}

impl ChainListener {
    /// Create a new [`Self`] instance.
    ///
    /// # Arguments
    /// - `rpc_url` - The Solana RPC endpoint URL
    /// - `ics26_router_program_id` - The ICS26 Router program ID on Solana
    pub fn new(rpc_url: String, ics26_router_program_id: Pubkey) -> Self {
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            rpc_url,
            CommitmentConfig::confirmed(),
        ));

        Self {
            rpc_client,
            ics26_router_program_id,
        }
    }

    /// Get the RPC client.
    pub fn client(&self) -> &Arc<RpcClient> {
        &self.rpc_client
    }

    /// Parse IBC events from Solana transaction logs.
    fn parse_events_from_logs(
        &self,
        logs: &[String],
        slot: u64,
    ) -> Result<Vec<SolanaEurekaEventWithHeight>> {
        // Use the shared event parser from solana module
        let parsed_events = parse_events_from_logs(logs)?;

        Ok(parsed_events
            .into_iter()
            .map(|event| SolanaEurekaEventWithHeight {
                event,
                height: slot,
            })
            .collect())
    }
}

#[async_trait::async_trait]
impl ChainListenerService<SolanaEureka> for ChainListener {
    async fn fetch_tx_events(
        &self,
        tx_ids: Vec<Signature>,
    ) -> Result<Vec<SolanaEurekaEventWithHeight>> {
        let mut all_events = Vec::new();

        for sig in tx_ids {
            // Fetch transaction with logs using get_transaction_with_config
            let config = solana_client::rpc_config::RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Base64),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            };

            match self.rpc_client.get_transaction_with_config(&sig, config) {
                Ok(tx) => {
                    let slot = tx.slot;

                    // Extract logs from transaction metadata
                    if let Some(meta) = &tx.transaction.meta {
                        match &meta.log_messages {
                            solana_transaction_status::option_serializer::OptionSerializer::Some(logs) => {
                                // Parse events from logs
                                match self.parse_events_from_logs(logs, slot) {
                                    Ok(events) => all_events.extend(events),
                                    Err(e) => {
                                        tracing::error!("Failed to parse events from transaction {}: {}", sig, e);
                                        // Continue processing other transactions
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch transaction {}: {}", sig, e);
                    // Continue with other transactions
                }
            }
        }

        Ok(all_events)
    }

    async fn fetch_events(
        &self,
        start_height: u64,
        end_height: u64,
    ) -> Result<Vec<SolanaEurekaEventWithHeight>> {
        // For Solana, we need to fetch blocks in the range and extract events
        let mut all_events = Vec::new();

        // Solana doesn't have a direct way to query events by block range,
        // so we need to fetch blocks and look for transactions to our program
        for slot in start_height..=end_height {
            // Get block with transaction details
            let block = match self.rpc_client.get_block_with_config(
                slot,
                solana_client::rpc_config::RpcBlockConfig {
                    encoding: Some(UiTransactionEncoding::Base64),
                    max_supported_transaction_version: Some(0),
                    rewards: Some(false),
                    commitment: Some(CommitmentConfig::confirmed()),
                    transaction_details: Some(solana_transaction_status::TransactionDetails::Full),
                },
            ) {
                Ok(block) => block,
                Err(e) => {
                    // Skip slots that don't exist (empty slots are common in Solana)
                    tracing::debug!("Skipping slot {}: {}", slot, e);
                    continue;
                }
            };

            // Process transactions in the block
            if let Some(transactions) = block.transactions {
                for tx_with_meta in transactions {
                    // Check if transaction involves our ICS26 router program
                    // Extract logs from transaction metadata
                    if let Some(meta) = &tx_with_meta.meta {
                        // solana_transaction_status uses OptionSerializer for optional fields
                        match &meta.log_messages {
                            solana_transaction_status::option_serializer::OptionSerializer::Some(logs) => {
                                // Check if any log mentions our program
                                let involves_ibc = logs.iter().any(|log|
                                    log.contains(&self.ics26_router_program_id.to_string())
                                );

                                if involves_ibc {
                                    match self.parse_events_from_logs(logs, slot) {
                                        Ok(events) => all_events.extend(events),
                                        Err(e) => {
                                            tracing::error!("Failed to parse events from block {} transaction: {}", slot, e);
                                            // Continue processing other transactions
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        Ok(all_events)
    }
}

