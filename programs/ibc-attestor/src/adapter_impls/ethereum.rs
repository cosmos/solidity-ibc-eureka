mod config;

use super::common_evm::{ChainLabels, EvmClient};
use crate::adapter_client::{AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation};
use crate::AttestorError;

pub use config::EthClientConfig;

#[derive(Debug)]
pub struct EthClient(EvmClient);

impl EthClient {
    pub fn from_config(config: &EthClientConfig) -> Result<Self, AttestorError> {
        let labels = ChainLabels {
            block_label: "L1",
            packet_label: "Ethereum L1",
            log_name: "ethereum",
        };
        Ok(Self(EvmClient::new(&config.url, &config.router_address, labels)?))
    }
}

impl AttestationAdapter for EthClient {
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
