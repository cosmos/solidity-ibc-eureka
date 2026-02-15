//! Transaction builder for Solana-to-Eth relay using attestation proofs.
//!
//! Delegates to `eth_attested` utilities for building ABI-encoded EVM
//! multicall transactions. Payloads from Solana are already ABI-encoded
//! by the IFT program, so no translation is needed (passthrough).

use std::collections::HashMap;

use alloy::providers::RootProvider;
use anyhow::Result;
use ibc_eureka_relayer_lib::{
    aggregator::{Aggregator, Config as AggregatorConfig},
    utils::{
        eth_attested::{
            build_eth_attestor_create_client_calldata, build_eth_attestor_relay_events_tx,
            build_eth_attestor_update_client_calldata,
        },
        RelayEventsParams,
    },
};

/// Transaction builder using attestation proofs for Solana-to-Eth relay.
pub struct AttestedTxBuilder {
    aggregator: Aggregator,
    provider: RootProvider,
}

impl AttestedTxBuilder {
    /// Create a new [`AttestedTxBuilder`] instance.
    pub async fn new(aggregator_config: AggregatorConfig, provider: RootProvider) -> Result<Self> {
        let aggregator = Aggregator::from_config(aggregator_config).await?;
        Ok(Self {
            aggregator,
            provider,
        })
    }

    /// Relay events from Solana to Ethereum using attestations.
    ///
    /// Builds an ABI-encoded multicall transaction containing update_client,
    /// recv_packet, ack_packet, and timeout_packet calls.
    pub async fn relay_events(&self, params: RelayEventsParams) -> Result<Vec<u8>> {
        build_eth_attestor_relay_events_tx(&self.aggregator, params).await
    }

    /// Build create_client calldata for an attestation light client on EVM.
    pub fn create_client(&self, parameters: &HashMap<String, String>) -> Result<Vec<u8>> {
        build_eth_attestor_create_client_calldata(parameters, self.provider.clone())
    }

    /// Build update_client calldata for the attestation light client on EVM.
    pub async fn update_client(&self, dst_client_id: &str) -> Result<Vec<u8>> {
        build_eth_attestor_update_client_calldata(&self.aggregator, dst_client_id.to_string()).await
    }
}
