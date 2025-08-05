//! This module defines [`TxBuilder`] which is responsible for building transactions to be sent to
//! the Cosmos SDK chain from events received from an Attested chain via the aggregator.

use std::collections::HashMap;

use anyhow::Result;
use ibc_proto_eureka::cosmos::tx::v1beta1::TxBody;
use prost::Message;
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

        let mut aggregator_client = self.create_aggregator_client().await?;

        // Get the stored tx IDs for aggregator query
        let src_tx_ids = self.src_tx_ids.lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?
            .clone();
        let timeout_tx_ids = self.timeout_tx_ids.lock()
            .map_err(|e| anyhow::anyhow!("Lock error: {}", e))?
            .clone();

        if src_tx_ids.is_empty() && timeout_tx_ids.is_empty() {
            anyhow::bail!("No transaction IDs provided for aggregator query");
        }

        // Convert tx IDs to bytes for the aggregator request
        let mut packets = Vec::new();
        for tx_id in &src_tx_ids {
            packets.push(hex::decode(tx_id)?);
        }
        for tx_id in &timeout_tx_ids {
            packets.push(hex::decode(tx_id)?);
        }

        // Make aggregator call with latest height (0 for latest)
        let request = aggregator_proto::GetStateAttestationRequest {
            packets,
            height: 0, // 0 means latest height
        };

        tracing::info!("Requesting state attestation from aggregator for {} packets", request.packets.len());
        
        let response = aggregator_client
            .get_state_attestation(request)
            .await?
            .into_inner();

        tracing::info!(
            "Received state attestation: {} signatures, height {}, state: {}",
            response.sig_pubkey_pairs.len(),
            response.height,
            hex::encode(&response.state)
        );

        // TODO: Build actual cosmos transaction with the attestation data
        // This requires implementing the IBC client and packet message construction
        let tx_body = TxBody {
            messages: vec![],
            ..Default::default()
        };

        let serialized = tx_body.encode_to_vec();
        Ok(serialized)
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        tracing::info!("Creating attested light client");

        if !parameters.is_empty() {
            anyhow::bail!("Parameters are not supported for creating an attested light client");
        }

        let mut aggregator_client = self.create_aggregator_client().await?;

        // For client creation, we get the current state attestation with no specific packets
        let request = aggregator_proto::GetStateAttestationRequest {
            packets: vec![], // Empty packets for client creation
            height: 0, // Latest height
        };

        tracing::info!("Requesting initial state attestation for client creation");
        
        let response = aggregator_client
            .get_state_attestation(request)
            .await?
            .into_inner();

        tracing::info!(
            "Received initial state attestation: {} signatures, height {}, state: {}",
            response.sig_pubkey_pairs.len(),
            response.height,
            hex::encode(&response.state)
        );

        // TODO: Build actual MsgCreateClient with the attestation data
        // This requires implementing the attested client state and consensus state construction
        let tx_body = TxBody {
            messages: vec![],
            ..Default::default()
        };

        let serialized = tx_body.encode_to_vec();
        Ok(serialized)
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(&self, dst_client_id: String) -> Result<Vec<u8>> {
        tracing::info!("Updating attested light client: {}", dst_client_id);

        let mut aggregator_client = self.create_aggregator_client().await?;

        // For client update, we get the current state attestation with no specific packets
        let request = aggregator_proto::GetStateAttestationRequest {
            packets: vec![], // Empty packets for client update
            height: 0, // Latest height
        };

        tracing::info!("Requesting current state attestation for client update");
        
        let response = aggregator_client
            .get_state_attestation(request)
            .await?
            .into_inner();

        tracing::info!(
            "Received state attestation for client {}: {} signatures, height {}, state: {}",
            dst_client_id,
            response.sig_pubkey_pairs.len(),
            response.height,
            hex::encode(&response.state)
        );

        // TODO: Build actual MsgUpdateClient with the attestation data
        // This requires implementing the attested consensus state construction
        let tx_body = TxBody {
            messages: vec![],
            ..Default::default()
        };

        let serialized = tx_body.encode_to_vec();
        Ok(serialized)
    }
}
