use alloy::{
    consensus::BlockHeader,
    eips::{BlockId, BlockNumberOrTag},
    hex::FromHex,
    sol_types::SolValue,
};
use alloy_network::Ethereum;
use alloy_primitives::{keccak256, Address, FixedBytes};
use alloy_provider::{Provider, RootProvider};

use attestor_packet_membership::Packets;
use futures::{stream::FuturesUnordered, StreamExt};
use ibc_eureka_solidity_types::ics26::{router::routerInstance, IICS26RouterMsgs::Packet};

use crate::adapter_client::{AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation};
use crate::AttestorError;

#[derive(Clone, Copy, Debug)]
pub struct ChainLabels {
    /// Label for missing block errors (e.g., "L1", "Arbitrum", "OP")
    pub block_label: &'static str,
    /// Label for packet-not-found errors (e.g., "Ethereum L1", "Arbitrum L2", "OP L2")
    pub packet_label: &'static str,
    /// Lowercase name for log messages (e.g., "ethereum", "arbitrum", "optimism")
    pub log_name: &'static str,
}

#[derive(Debug)]
pub struct EvmClient {
    client: RootProvider,
    router: routerInstance<RootProvider>,
    labels: ChainLabels,
}

impl EvmClient {
    pub fn new(url: &str, router_address: &str, labels: ChainLabels) -> Result<Self, AttestorError> {
        let url = url
            .parse()
            .map_err(|_| AttestorError::ClientConfigError(format!("url {} could not be parsed", url)))?;

        let client = RootProvider::<Ethereum>::new_http(url);

        let address = Address::from_hex(router_address)
            .map_err(|e| AttestorError::ClientConfigError(e.to_string()))?;

        let router = routerInstance::new(address.into(), client.clone());

        Ok(Self { client, router, labels })
    }

    async fn get_timestamp_for_block_at_height(&self, height: u64) -> Result<u64, AttestorError> {
        self.client
            .get_block_by_number(BlockNumberOrTag::Number(height))
            .await
            .map_err(|e| AttestorError::ClientError(e.to_string()))?
            .ok_or_else(|| AttestorError::ClientError(format!("no {} block of kind {} found", self.labels.block_label, height)))
            .map(|header| header.header.timestamp())
    }

    async fn get_historical_packet_commitment(
        &self,
        hashed_path: FixedBytes<32>,
        block_number: u64,
    ) -> Result<[u8; 32], AttestorError> {
        let cmt = self
            .router
            .getCommitment(hashed_path)
            .block(BlockId::Number(BlockNumberOrTag::Number(block_number)))
            .call()
            .await
            .map_err(|e| AttestorError::ClientError(e.to_string()))?;

        let is_empty = cmt.iter().max() == Some(&0);
        if is_empty {
            Err(AttestorError::ClientError(format!(
                "commitment path {:?} at height {} not found in {}",
                hashed_path, block_number, self.labels.packet_label
            )))
        } else {
            Ok(*cmt)
        }
    }
}

impl AttestationAdapter for EvmClient {
    async fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> Result<UnsignedStateAttestation, AttestorError> {
        let ts = self.get_timestamp_for_block_at_height(height).await?;
        Ok(UnsignedStateAttestation { height, timestamp: ts })
    }

    async fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &Packets,
        height: u64,
    ) -> Result<UnsignedPacketAttestation, AttestorError> {
        tracing::debug!(
            "Total {} packets received: {}",
            self.labels.log_name,
            packets.packets().count()
        );

        let mut futures = FuturesUnordered::new();
        for p in packets.packets() {
            let packet = Packet::abi_decode(p).map_err(AttestorError::DecodePacket)?;
            let validate_commitment = async move |packet: Packet, height: u64| {
                let commitment_path = packet.commitment_path();
                let hashed = keccak256(&commitment_path);
                let cmt = self.get_historical_packet_commitment(hashed, height).await?;
                if &packet.commitment() != &cmt {
                    Err(AttestorError::InvalidCommitment {
                        reason: "requested and received packet commitments do not match".into(),
                    })
                } else {
                    Ok(cmt)
                }
            };
            futures.push(validate_commitment(packet, height));
        }

        let mut validated = Vec::with_capacity(futures.len());
        while let Some(maybe_cmt) = futures.next().await {
            match maybe_cmt {
                Ok(cmt) => validated.push(cmt),
                Err(e) => return Err(e),
            }
        }

        tracing::debug!(
            "Total {} packets validated : {}",
            self.labels.log_name,
            validated.len()
        );

        Ok(UnsignedPacketAttestation { height, packets: validated })
    }
}


