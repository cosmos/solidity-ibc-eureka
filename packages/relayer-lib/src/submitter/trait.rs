use crate::chain::Chain;

use anyhow::Result;

/// The `ChainSubmitterService` trait defines the interface for a service that submits transactions
/// to a chain based on events from two chains.
#[async_trait::async_trait]
#[allow(dead_code)]
pub trait ChainSubmitterService<A: Chain, B: Chain> {
    /// Submit a transaction to chain A based on the events from chain A and chain B.
    /// Events from chain A are often used for timeout purposes and can be left empty.
    async fn submit_events(
        &self,
        a_events: Vec<A::Event>,
        b_events: Vec<B::Event>,
    ) -> Result<A::TxId>;
}
