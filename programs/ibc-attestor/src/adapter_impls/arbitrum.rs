mod config;

use super::common_evm::{ChainLabels, EvmClient};
use crate::adapter_client::{AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation};
use crate::AttestorError;

pub use config::ArbitrumClientConfig;

#[derive(Debug)]
pub struct ArbitrumClient(EvmClient);

impl ArbitrumClient {
    pub fn from_config(config: &ArbitrumClientConfig) -> Result<Self, AttestorError> {
        let labels = ChainLabels {
            block_label: "Arbitrum",
            packet_label: "Arbitrum L2",
            log_name: "arbitrum",
        };
        Ok(Self(EvmClient::new(&config.url, &config.router_address, labels)?))
    }
}

impl AttestationAdapter for ArbitrumClient {
    async fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> Result<UnsignedStateAttestation, AttestorError> {
        self.0.get_unsigned_state_attestation_at_height(height).await
    }

    async fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &attestor_packet_membership::Packets,
        height: u64,
    ) -> Result<UnsignedPacketAttestation, AttestorError> {
        self.0
            .get_unsigned_packet_attestation_at_height(packets, height)
            .await
    }
}
