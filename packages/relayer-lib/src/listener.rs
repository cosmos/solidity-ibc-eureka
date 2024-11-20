//! Defines the [`ChainListenerService`] trait.

use anyhow::Result;
use serde::{de::DeserializeOwned, Serialize};
use std::fmt::Debug;

/// The `ChainListenerService` trait defines the interface for a service that listens to a chain
#[async_trait::async_trait]
pub trait ChainListenerService {
    /// The event type that the listener will return.
    /// These should be the events that the relayer is interested in.
    type Event: Clone + Serialize + DeserializeOwned + Debug;
    /// The transaction identifier type that the listener will ask for.
    /// This is often a hash of the transaction.
    type TxId: Clone + Serialize + DeserializeOwned + Debug;
    /// The block height type that the listener will ask for.
    /// This is often a u64.
    type Height: Clone + Serialize + DeserializeOwned + Debug + std::cmp::PartialOrd;

    /// Fetch events from a transaction.
    async fn fetch_tx_events(&self, tx_id: &Self::TxId) -> Result<Vec<Self::Event>>;

    /// Fetch events from a block range.
    /// Both the start and end heights are inclusive.
    async fn fetch_events(
        &self,
        start_height: Self::Height,
        end_height: Self::Height,
    ) -> Result<Vec<Self::Event>>;
}
