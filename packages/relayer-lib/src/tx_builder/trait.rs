use crate::chain::Chain;
use anyhow::Result;
use std::collections::HashMap;

/// The `TxBuilderService` trait defines the interface for a service that submits transactions
/// to a chain based on events from two chains.
#[async_trait::async_trait]
pub trait TxBuilderService<A: Chain, B: Chain> {
    /// Generate a transaction to chain A based on the events from chain A and chain B.
    /// Events from chain A are often used for timeout purposes and can be left empty.
    ///
    /// # Arguments
    /// - `src_events` - The events from chain B.
    /// - `target_events` - The events from chain A.
    /// - `src_client_id` - The client ID on chain B.
    /// - `dst_client_id` - The client ID on chain A.
    /// - `src_packet_seqs` - The packets to relay on chain B (from events). All packets are
    /// relayed if empty.
    /// - `dst_packet_seqs` - The packets to relay on chain A (from events). All packets are
    /// relayed if empty.
    ///
    /// # Returns
    /// The relay transaction bytes.
    async fn relay_events(
        &self,
        src_events: Vec<A::Event>,
        target_events: Vec<B::Event>,
        src_client_id: String,
        dst_client_id: String,
        src_packet_seqs: Vec<u64>,
        dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<u8>>;

    /// Create a transaction to chain A that creates a light client of chain B.
    ///
    /// # Arguments
    /// - `parameters` - The optional parameters for the light client creation.
    ///
    /// # Returns
    /// The relay transaction bytes.
    async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>>;
}
