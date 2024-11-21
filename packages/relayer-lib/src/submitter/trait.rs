use crate::chain::Chain;

use anyhow::Result;

/// The `ChainSubmitterService` trait defines the interface for a service that submits transactions
/// to a chain based on events from another chain.
#[async_trait::async_trait]
#[allow(dead_code)]
pub trait ChainSubmitterService<A: Chain, B: Chain> {
    /// Submit a transaction to chain A based on the events from chain B.
    async fn submit_events(&self, events: Vec<B::Event>) -> Result<A::TxId>;
}
