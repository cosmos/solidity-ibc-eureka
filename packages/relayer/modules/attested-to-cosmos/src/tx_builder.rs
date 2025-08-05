//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from an Attested chain via the aggregator.

use std::collections::HashMap;

use anyhow::Result;
use ibc_proto_eureka::cosmos::tx::v1beta1::TxBody;
use tendermint_rpc::HttpClient;
use tonic::transport::Channel;

use ibc_eureka_relayer_lib::{
    chain::{Chain, CosmosSdk},
    events::EurekaEventWithHeight,
    tx_builder::TxBuilderService,
};

/// Chain type for attested chains that get their state from the aggregator
pub struct AttestedChain;

impl Chain for AttestedChain {
    type Event = EurekaEventWithHeight;
    type TxId = String;
    type Height = u64;
}

/// Generated aggregator client protobuf definitions
pub mod aggregator_proto {
    tonic::include_proto!("aggregator");
}

/// The aggregator client for fetching attestations.
pub type AggregatorClient = aggregator_proto::aggregator_client::AggregatorClient<Channel>;

/// The `TxBuilder` produces txs to [`CosmosSdk`] based on attestations from the aggregator.
pub struct TxBuilder {
    /// The aggregator URL for fetching attestations.
    pub aggregator_url: String,
    /// The HTTP client for the target chain.
    pub target_tm_client: HttpClient,
    /// The signer address for the Cosmos messages.
    pub signer_address: String,
    /// Cached source tx IDs for use in aggregator queries
    pub src_tx_ids: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// Cached timeout tx IDs for use in aggregator queries
    pub timeout_tx_ids: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl TxBuilder {
    /// Creates a new `TxBuilder`.
    #[must_use]
    pub fn new(
        aggregator_url: String,
        target_tm_client: HttpClient,
        signer_address: String,
    ) -> Self {
        Self {
            aggregator_url,
            target_tm_client,
            signer_address,
            src_tx_ids: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
            timeout_tx_ids: std::sync::Arc::new(std::sync::Mutex::new(vec![])),
        }
    }

    /// Sets the transaction IDs to be used for aggregator queries
    ///
    /// # Errors
    /// Returns error if mutex lock fails
    pub fn set_tx_ids(&self, src_tx_ids: Vec<String>, timeout_tx_ids: Vec<String>) -> Result<()> {
        *self
            .src_tx_ids
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))? = src_tx_ids;
        *self
            .timeout_tx_ids
            .lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))? = timeout_tx_ids;
        Ok(())
    }

    /// Creates an aggregator client.
    async fn create_aggregator_client(&self) -> Result<AggregatorClient> {
        let channel = Channel::from_shared(self.aggregator_url.clone())?
            .connect()
            .await?;
        Ok(AggregatorClient::new(channel))
    }
}

#[async_trait::async_trait]
impl TxBuilderService<AttestedChain, CosmosSdk> for TxBuilder {
    #[tracing::instrument(skip_all)]
    async fn relay_events(
        &self,
        _src_events: Vec<EurekaEventWithHeight>,
        _target_events: Vec<EurekaEventWithHeight>,
        _src_client_id: String,
        _dst_client_id: String,
        _src_packet_seqs: Vec<u64>,
        _dst_packet_seqs: Vec<u64>,
    ) -> Result<Vec<u8>> {
        tracing::info!(
            "Building relay transaction from aggregator for {} source events and {} timeout events",
            _src_events.len(),
            _target_events.len()
        );

        let mut _aggregator_client = self.create_aggregator_client().await?;

        // TODO: Implement aggregator call to get both update client and packet data
        // This will be implemented when IBC-135 is completed

        // Placeholder for now - return empty transaction body
        let _tx_body = TxBody {
            messages: vec![],
            ..Default::default()
        };

        anyhow::bail!("Aggregator integration not yet implemented - waiting for IBC-135");
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        tracing::info!("Creating attested light client");

        if !parameters.is_empty() {
            anyhow::bail!("Parameters are not supported for creating an attested light client");
        }

        let mut _aggregator_client = self.create_aggregator_client().await?;

        // TODO: Implement aggregator call to get initial state attestation for client creation
        // This will be implemented when IBC-135 is completed

        // Placeholder for now - return empty transaction body
        let _tx_body = TxBody {
            messages: vec![],
            ..Default::default()
        };

        anyhow::bail!("Aggregator integration not yet implemented - waiting for IBC-135");
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(&self, dst_client_id: String) -> Result<Vec<u8>> {
        tracing::info!("Updating attested light client: {}", dst_client_id);

        let mut _aggregator_client = self.create_aggregator_client().await?;

        // TODO: Implement aggregator call to get state attestation for client update
        // This will be implemented when IBC-135 is completed

        // Placeholder for now - return empty transaction body
        let _tx_body = TxBody {
            messages: vec![],
            ..Default::default()
        };

        anyhow::bail!("Aggregator integration not yet implemented - waiting for IBC-135");
    }
}
