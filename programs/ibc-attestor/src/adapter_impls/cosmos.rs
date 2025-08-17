mod config;
use alloy::sol_types::SolValue;

use attestor_packet_membership::Packets;
use futures::{stream::FuturesUnordered, StreamExt};
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet as SolidityPacket;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use tendermint_rpc::HttpClient;

use crate::adapter_client::{AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation};
use crate::AttestorError;

pub use config::CosmosClientConfig;

#[derive(Clone, Copy, Debug)]
struct ChainLabels {
    block_label: &'static str,
    packet_label: &'static str,
    log_name: &'static str,
}

trait Backend: Clone + Send + Sync + 'static {
    fn new(url: &str, store_prefix: &str) -> Result<Self, AttestorError>;
    fn labels(&self) -> ChainLabels;
    fn with_labels(self, labels: ChainLabels) -> Self;
    fn store_prefix(&self) -> &str;

    fn block_timestamp<'a>(&'a self, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<u64>, AttestorError>> + Send + 'a>>;
    fn value_at_path<'a>(&'a self, key: Vec<u8>, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<Vec<u8>>, AttestorError>> + Send + 'a>>;
}

#[derive(Clone, Debug)]
struct ProdBackend {
    client: HttpClient,
    store_prefix: String,
    labels: ChainLabels,
}

impl Backend for ProdBackend {
    fn new(url: &str, store_prefix: &str) -> Result<Self, AttestorError> {
        Ok(Self {
            client: HttpClient::from_rpc_url(url),
            store_prefix: store_prefix.to_string(),
            labels: ChainLabels { block_label: "", packet_label: "", log_name: "" },
        })
    }

    fn labels(&self) -> ChainLabels { self.labels }
    fn with_labels(mut self, labels: ChainLabels) -> Self { self.labels = labels; self }
    fn store_prefix(&self) -> &str { &self.store_prefix }

    fn block_timestamp<'a>(&'a self, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<u64>, AttestorError>> + Send + 'a>> {
        Box::pin(async move {
            let lb = self
                .client
                .get_light_block(Some(height))
                .await
                .map_err(|e| AttestorError::ClientError(e.to_string()))?;
            let ts = lb.signed_header.header().time.unix_timestamp();
            if ts < 0 { Ok(None) } else { Ok(Some(ts as u64)) }
        })
    }

    fn value_at_path<'a>(&'a self, key: Vec<u8>, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<Vec<u8>>, AttestorError>> + Send + 'a>> {
        Box::pin(async move {
            let path = vec![self.store_prefix().as_bytes().to_vec(), key];
            let (value, _proof) = self
                .client
                .prove_path(&path, height)
                .await
                .map_err(|e| AttestorError::ClientError(e.to_string()))?;
            if value.is_empty() { Ok(None) } else { Ok(Some(value)) }
        })
    }
}

#[derive(Debug)]
pub struct CosmosClientInner<B: Backend> {
    backend: B,
}

impl<B: Backend> CosmosClientInner<B> {
    pub fn new(url: &str, store_prefix: &str, labels: ChainLabels) -> Result<Self, AttestorError> {
        let backend = B::new(url, store_prefix)?.with_labels(labels);
        Ok(Self { backend })
    }

    fn labels(&self) -> ChainLabels { self.backend.labels() }

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

    async fn get_historical_packet_commitment(&self, path: Vec<u8>, block_number: u64) -> Result<[u8; 32], AttestorError> {
        match self.backend.value_at_path(path, block_number).await? {
            Some(value) => {
                if value.len() != 32 {
                    return Err(AttestorError::ClientError("commitment value length is not 32 bytes".into()));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&value);
                Ok(arr)
            }
            None => Err(AttestorError::ClientError(format!(
                "commitment at height {} not found in {}",
                block_number, self.labels().packet_label
            ))),
        }
    }
}

impl<B: Backend> AttestationAdapter for CosmosClientInner<B> {
    async fn get_unsigned_state_attestation_at_height(&self, height: u64) -> Result<UnsignedStateAttestation, AttestorError> {
        let ts = self.get_timestamp_for_block_at_height(height).await?;
        Ok(UnsignedStateAttestation { height, timestamp: ts })
    }

    async fn get_unsigned_packet_attestation_at_height(&self, packets: &Packets, height: u64) -> Result<UnsignedPacketAttestation, AttestorError> {
        let labels = self.labels();
        tracing::debug!("Total {} packets received: {}", labels.log_name, packets.packets().count());

        let mut futures = FuturesUnordered::new();
        for p in packets.packets() {
            let packet = SolidityPacket::abi_decode(p).map_err(AttestorError::DecodePacket)?;
            let validate_commitment = async move |packet: SolidityPacket, height: u64| {
                let commitment_path = packet.commitment_path();
                let cmt = self.get_historical_packet_commitment(commitment_path, height).await?;
                let pkt_commitment = packet.commitment();
                if pkt_commitment.as_slice() != &cmt {
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
pub struct CosmosClient(pub(crate) CosmosClientInner<ProdBackend>);

impl CosmosClient {
    pub fn from_config(config: &CosmosClientConfig) -> Result<Self, AttestorError> {
        let labels = ChainLabels {
            block_label: "Tendermint",
            packet_label: "Cosmos IBC store",
            log_name: "cosmos",
        };
        Ok(Self(CosmosClientInner::<ProdBackend>::new(&config.url, &config.store_prefix, labels)?))
    }
}

impl AttestationAdapter for CosmosClient {
    async fn get_unsigned_state_attestation_at_height(&self, height: u64) -> Result<UnsignedStateAttestation, AttestorError> {
        self.0.get_unsigned_state_attestation_at_height(height).await
    }

    async fn get_unsigned_packet_attestation_at_height(&self, packets: &Packets, height: u64) -> Result<UnsignedPacketAttestation, AttestorError> {
        self.0.get_unsigned_packet_attestation_at_height(packets, height).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use attestor_packet_membership::Packets;
    use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet as SolidityPacket;
    use std::collections::HashMap;

    #[derive(Clone, Debug)]
    struct MockBackend {
        labels: ChainLabels,
        store_prefix: String,
        block_ts: HashMap<u64, Option<u64>>,
        values: HashMap<(Vec<u8>, u64), Option<Vec<u8>>>,
    }

    impl Default for MockBackend {
        fn default() -> Self {
            Self {
                labels: ChainLabels { block_label: "", packet_label: "", log_name: "" },
                store_prefix: "ibc".to_string(),
                block_ts: HashMap::new(),
                values: HashMap::new(),
            }
        }
    }

    impl Backend for MockBackend {
        fn new(_: &str, _: &str) -> Result<Self, AttestorError> { Ok(Self::default()) }
        fn labels(&self) -> ChainLabels { self.labels }
        fn with_labels(mut self, labels: ChainLabels) -> Self { self.labels = labels; self }
        fn store_prefix(&self) -> &str { &self.store_prefix }
        fn block_timestamp<'a>(&'a self, height: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<u64>, AttestorError>> + Send + 'a>> {
            Box::pin(async move { Ok(self.block_ts.get(&height).cloned().unwrap_or(None)) })
        }
        fn value_at_path<'a>(&'a self, key: Vec<u8>, block_number: u64) -> core::pin::Pin<Box<dyn core::future::Future<Output = Result<Option<Vec<u8>>, AttestorError>> + Send + 'a>> {
            Box::pin(async move { Ok(self.values.get(&(key, block_number)).cloned().unwrap_or(None)) })
        }
    }

    fn labels_cosmos() -> ChainLabels { ChainLabels { block_label: "Tendermint", packet_label: "Cosmos IBC store", log_name: "cosmos" } }

    fn make_packet_bytes(seq: u64, src: &str, dst: &str, timeout: u64) -> Vec<u8> {
        let pkt = SolidityPacket { sequence: seq, sourceClient: src.into(), destClient: dst.into(), timeoutTimestamp: timeout, payloads: vec![] };
        pkt.abi_encode()
    }

    fn derive_path_and_commitment(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let pkt = SolidityPacket::abi_decode(bytes).unwrap();
        let path = pkt.commitment_path();
        let commitment = pkt.commitment();
        (path, commitment)
    }

    #[tokio::test]
    async fn happy_path_single_packet() {
        let labels = labels_cosmos();
        let mut mb = MockBackend::default().with_labels(labels);
        mb.block_ts.insert(10, Some(1111));
        let pktb = make_packet_bytes(1, "src", "dst", 0);
        let (path, c) = derive_path_and_commitment(&pktb);
        mb.values.insert((path, 10), Some(c.clone()));
        let client = CosmosClientInner::<MockBackend> { backend: mb };
        let pkts = Packets::new(vec![pktb]);
        let state = client.get_unsigned_state_attestation_at_height(10).await.unwrap();
        assert_eq!(state.timestamp, 1111);
        let res = client.get_unsigned_packet_attestation_at_height(&pkts, 10).await.unwrap();
        assert_eq!(res.packets.len(), 1);
    }

    #[tokio::test]
    async fn invalid_commitment() {
        let labels = labels_cosmos();
        let mut mb = MockBackend::default().with_labels(labels);
        mb.block_ts.insert(10, Some(1111));
        let pktb = make_packet_bytes(1, "src", "dst", 0);
        let (path, _c) = derive_path_and_commitment(&pktb);
        // Return a different commitment
        mb.values.insert((path, 10), Some(vec![1u8; 32]));
        let client = CosmosClientInner::<MockBackend> { backend: mb };
        let pkts = Packets::new(vec![pktb]);
        let err = client.get_unsigned_packet_attestation_at_height(&pkts, 10).await.err().unwrap();
        assert!(err.to_string().contains("requested and received packet commitments do not match"));
    }

    #[tokio::test]
    async fn missing_commitment_has_label() {
        let labels = labels_cosmos();
        let mut mb = MockBackend::default().with_labels(labels);
        mb.block_ts.insert(10, Some(1111));
        let pktb = make_packet_bytes(1, "src", "dst", 0);
        let (path, _c) = derive_path_and_commitment(&pktb);
        mb.values.insert((path, 10), None);
        let client = CosmosClientInner::<MockBackend> { backend: mb };
        let pkts = Packets::new(vec![pktb]);
        let err = client.get_unsigned_packet_attestation_at_height(&pkts, 10).await.err().unwrap();
        assert!(err.to_string().contains("Cosmos IBC store"));
    }

    #[tokio::test]
    async fn missing_block_has_label() {
        let labels = labels_cosmos();
        let mb = MockBackend::default().with_labels(labels);
        let client = CosmosClientInner::<MockBackend> { backend: mb };
        let err = client.get_unsigned_state_attestation_at_height(99).await.err().unwrap();
        assert!(err.to_string().contains("no Tendermint block"));
    }
}


