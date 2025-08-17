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

/// Abstraction over how to fetch timestamps and commitments.
trait Backend: Clone + Send + Sync + 'static {
    fn new(url: &str, router_address: &str) -> Result<Self, AttestorError>;
    fn labels(&self) -> ChainLabels;
    fn with_labels(self, labels: ChainLabels) -> Self;
    fn get_labels_ref(&self) -> &ChainLabels;
    fn get_labels_mut(&mut self) -> &mut ChainLabels;
    fn into_labels(self) -> ChainLabels;

    fn set_labels(&mut self, labels: ChainLabels);

    fn block_timestamp<'a>(&'a self, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<u64>, AttestorError>> + Send + 'a>>;
    fn commitment<'a>(&'a self, hashed_path: FixedBytes<32>, block_number: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<[u8; 32]>, AttestorError>> + Send + 'a>>;
}

#[derive(Clone, Debug)]
struct ProdBackend {
    client: RootProvider,
    router: routerInstance<RootProvider>,
    labels: ChainLabels,
}

impl Backend for ProdBackend {
    fn new(url: &str, router_address: &str) -> Result<Self, AttestorError> {
        let parsed = url
            .parse()
            .map_err(|_| AttestorError::ClientConfigError(format!("url {} could not be parsed", url)))?;
        let client = RootProvider::<Ethereum>::new_http(parsed);
        let address = Address::from_hex(router_address)
            .map_err(|e| AttestorError::ClientConfigError(e.to_string()))?;
        let router = routerInstance::new(address.into(), client.clone());
        Ok(Self { client, router, labels: ChainLabels { block_label: "", packet_label: "", log_name: "" } })
    }

    fn labels(&self) -> ChainLabels { self.labels }
    fn with_labels(mut self, labels: ChainLabels) -> Self { self.labels = labels; self }
    fn get_labels_ref(&self) -> &ChainLabels { &self.labels }
    fn get_labels_mut(&mut self) -> &mut ChainLabels { &mut self.labels }
    fn into_labels(self) -> ChainLabels { self.labels }
    fn set_labels(&mut self, labels: ChainLabels) { self.labels = labels; }

    fn block_timestamp<'a>(&'a self, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<u64>, AttestorError>> + Send + 'a>> {
        Box::pin(async move {
            let maybe = self
                .client
                .get_block_by_number(BlockNumberOrTag::Number(height))
                .await
                .map_err(|e| AttestorError::ClientError(e.to_string()))?;
            Ok(maybe.map(|h| h.header.timestamp()))
        })
    }

    fn commitment<'a>(&'a self, hashed_path: FixedBytes<32>, block_number: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<[u8; 32]>, AttestorError>> + Send + 'a>> {
        Box::pin(async move {
            let cmt = self
                .router
                .getCommitment(hashed_path)
                .block(BlockId::Number(BlockNumberOrTag::Number(block_number)))
                .call()
                .await
                .map_err(|e| AttestorError::ClientError(e.to_string()))?;
            let is_empty = cmt.iter().max() == Some(&0);
            if is_empty { Ok(None) } else { Ok(Some(*cmt)) }
        })
    }
}

#[derive(Debug)]
pub struct EvmClientInner<B: Backend> {
    backend: B,
}

impl<B: Backend> EvmClientInner<B> {
    pub fn new(url: &str, router_address: &str, labels: ChainLabels) -> Result<Self, AttestorError> {
        let backend = B::new(url, router_address)?.with_labels(labels);
        Ok(Self { backend })
    }

    fn labels(&self) -> ChainLabels { *self.backend.get_labels_ref() }

    async fn get_timestamp_for_block_at_height(&self, height: u64) -> Result<u64, AttestorError> {
        match self.backend.block_timestamp(height).await? {
            Some(ts) => Ok(ts),
            None => Err(AttestorError::ClientError(format!(
                "no {} block of kind {} found",
                self.labels().block_label,
                height
            ))),
        }
    }

    async fn get_historical_packet_commitment(&self, hashed_path: FixedBytes<32>, block_number: u64) -> Result<[u8; 32], AttestorError> {
        match self.backend.commitment(hashed_path, block_number).await? {
            Some(cmt) => Ok(cmt),
            None => Err(AttestorError::ClientError(format!(
                "commitment path {:?} at height {} not found in {}",
                hashed_path, block_number, self.labels().packet_label
            ))),
        }
    }
}

impl<B: Backend> AttestationAdapter for EvmClientInner<B> {
    async fn get_unsigned_state_attestation_at_height(&self, height: u64) -> Result<UnsignedStateAttestation, AttestorError> {
        let ts = self.get_timestamp_for_block_at_height(height).await?;
        Ok(UnsignedStateAttestation { height, timestamp: ts })
    }

    async fn get_unsigned_packet_attestation_at_height(&self, packets: &Packets, height: u64) -> Result<UnsignedPacketAttestation, AttestorError> {
        let labels = self.labels();
        tracing::debug!("Total {} packets received: {}", labels.log_name, packets.packets().count());

        let mut futures = FuturesUnordered::new();
        for p in packets.packets() {
            let packet = Packet::abi_decode(p).map_err(AttestorError::DecodePacket)?;
            let validate_commitment = async move |packet: Packet, height: u64| {
                let commitment_path = packet.commitment_path();
                let hashed = keccak256(&commitment_path);
                let cmt = self.get_historical_packet_commitment(hashed, height).await?;
                if &packet.commitment() != &cmt {
                    Err(AttestorError::InvalidCommitment { reason: "requested and received packet commitments do not match".into() })
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

        tracing::debug!("Total {} packets validated : {}", labels.log_name, validated.len());

        Ok(UnsignedPacketAttestation { height, packets: validated })
    }
}

#[derive(Debug)]
pub struct EvmClient(pub(crate) EvmClientInner<ProdBackend>);

impl EvmClient {
    pub fn new(url: &str, router_address: &str, labels: ChainLabels) -> Result<Self, AttestorError> {
        Ok(Self(EvmClientInner::<ProdBackend>::new(url, router_address, labels)?))
    }
}

impl AttestationAdapter for EvmClient {
    async fn get_unsigned_state_attestation_at_height(&self, height: u64) -> Result<UnsignedStateAttestation, AttestorError> {
        self.0.get_unsigned_state_attestation_at_height(height).await
    }

    async fn get_unsigned_packet_attestation_at_height(&self, packets: &Packets, height: u64) -> Result<UnsignedPacketAttestation, AttestorError> {
        self.0.get_unsigned_packet_attestation_at_height(packets, height).await
    }
}

#[cfg(test)]
pub mod test_utils {
    use super::*;
    use alloy_primitives::keccak256;
    use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet as SolPacket;
    use std::collections::HashMap;

    #[derive(Clone, Debug)]
    pub struct MockBackend {
        pub labels: ChainLabels,
        pub block_ts: HashMap<u64, Option<u64>>,
        pub cmts: HashMap<(FixedBytes<32>, u64), Option<[u8; 32]>>,
    }

    impl Default for MockBackend {
        fn default() -> Self {
            Self {
                labels: ChainLabels { block_label: "", packet_label: "", log_name: "" },
                block_ts: HashMap::new(),
                cmts: HashMap::new(),
            }
        }
    }

    impl Backend for MockBackend {
        fn new(_: &str, _: &str) -> Result<Self, AttestorError> { Ok(Self::default()) }
        fn labels(&self) -> ChainLabels { self.labels }
        fn with_labels(mut self, labels: ChainLabels) -> Self { self.labels = labels; self }
        fn get_labels_ref(&self) -> &ChainLabels { &self.labels }
        fn get_labels_mut(&mut self) -> &mut ChainLabels { &mut self.labels }
        fn into_labels(self) -> ChainLabels { self.labels }
        fn set_labels(&mut self, labels: ChainLabels) { self.labels = labels; }
        fn block_timestamp<'a>(&'a self, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<u64>, AttestorError>> + Send + 'a>> {
            Box::pin(async move { Ok(self.block_ts.get(&height).cloned().unwrap_or(None)) })
        }
        fn commitment<'a>(&'a self, hashed_path: FixedBytes<32>, block_number: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<[u8; 32]>, AttestorError>> + Send + 'a>> {
            Box::pin(async move { Ok(self.cmts.get(&(hashed_path, block_number)).cloned().unwrap_or(None)) })
        }
    }

    pub fn make_packet_bytes(seq: u64, src: &str, dst: &str, timeout: u64) -> Vec<u8> {
        let pkt = SolPacket { sequence: seq, sourceClient: src.into(), destClient: dst.into(), timeoutTimestamp: timeout, payloads: vec![] };
        pkt.abi_encode()
    }

    pub fn derive_hash_and_commitment(bytes: &[u8]) -> (FixedBytes<32>, [u8; 32]) {
        let pkt = SolPacket::abi_decode(bytes).unwrap();
        let path = pkt.commitment_path();
        let hashed = keccak256(&path);
        let mut c = [0u8; 32];
        c.copy_from_slice(&pkt.commitment());
        (hashed, c)
    }

    pub fn make_mock_client_with_backend(backend: MockBackend) -> super::EvmClientInner<MockBackend> {
        super::EvmClientInner { backend }
    }
}


