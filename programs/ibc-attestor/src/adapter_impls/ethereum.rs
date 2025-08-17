mod config;

use super::common_evm::{ChainLabels, EvmClient};
use crate::adapter_client::{AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation};
use crate::AttestorError;

pub use config::EthClientConfig;

#[derive(Debug)]
pub struct EthClient(EvmClient);

impl EthClient {
    pub fn from_config(config: &EthClientConfig) -> Result<Self, AttestorError> {
        Ok(Self(EvmClient::new(&config.url, &config.router_address, ETH_LABELS)?))
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(inner: super::common_evm::EvmClient) -> Self { Self(inner) }
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

pub(crate) const ETH_LABELS: ChainLabels = ChainLabels { block_label: "L1", packet_label: "Ethereum L1", log_name: "ethereum" };

#[cfg(test)]
mod tests {
    use super::*;
    use attestor_packet_membership::Packets;
    use crate::adapter_impls::common_evm::test_utils::{MockBackend, make_packet_bytes, derive_hash_and_commitment, make_mock_client_with_backend};

    #[tokio::test]
    async fn happy_path_single_packet_eth() {
        let mut mb = MockBackend::default();
        mb.labels = ETH_LABELS;
        mb.block_ts.insert(10, Some(1111));
        let pktb = make_packet_bytes(1, "src", "dst", 0);
        let (h, c) = derive_hash_and_commitment(&pktb);
        mb.cmts.insert((h, 10), Some(c));
        let client = make_mock_client_with_backend(mb);
        let pkts = Packets::new(vec![pktb]);
        let state = client.get_unsigned_state_attestation_at_height(10).await.unwrap();
        assert_eq!(state.timestamp, 1111);
        let res = client.get_unsigned_packet_attestation_at_height(&pkts, 10).await.unwrap();
        assert_eq!(res.packets.len(), 1);
    }

    #[tokio::test]
    async fn invalid_commitment_eth() {
        let mut mb = MockBackend::default();
        mb.labels = ETH_LABELS;
        mb.block_ts.insert(10, Some(1111));
        let pktb = make_packet_bytes(1, "src", "dst", 0);
        let (h, _c) = derive_hash_and_commitment(&pktb);
        mb.cmts.insert((h, 10), Some([1u8; 32]));
        let client = make_mock_client_with_backend(mb);
        let pkts = Packets::new(vec![pktb]);
        let err = client.get_unsigned_packet_attestation_at_height(&pkts, 10).await.err().unwrap();
        assert!(err.to_string().contains("requested and received packet commitments do not match"));
    }

    #[tokio::test]
    async fn missing_commitment_has_eth_label() {
        let mut mb = MockBackend::default();
        mb.labels = ETH_LABELS;
        mb.block_ts.insert(10, Some(1111));
        let pktb = make_packet_bytes(1, "src", "dst", 0);
        let (h, _c) = derive_hash_and_commitment(&pktb);
        mb.cmts.insert((h, 10), None);
        let client = make_mock_client_with_backend(mb);
        let pkts = Packets::new(vec![pktb]);
        let err = client.get_unsigned_packet_attestation_at_height(&pkts, 10).await.err().unwrap();
        assert!(err.to_string().contains("Ethereum L1"));
    }

    #[tokio::test]
    async fn missing_block_has_eth_label() {
        let mut mb = MockBackend::default();
        mb.labels = ETH_LABELS;
        let client = make_mock_client_with_backend(mb);
        let err = client.get_unsigned_state_attestation_at_height(99).await.err().unwrap();
        assert!(err.to_string().contains("no L1 block"));
    }
}
