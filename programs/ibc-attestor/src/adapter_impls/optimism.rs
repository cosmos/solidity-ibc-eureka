use alloy::{
    consensus::BlockHeader,
    eips::{BlockId, BlockNumberOrTag},
    hex::FromHex,
    sol_types::SolValue,
};
use alloy_network::Ethereum;
use alloy_primitives::{keccak256, Address, FixedBytes};
use alloy_provider::{Provider, RootProvider};

mod config;

use futures::{stream::FuturesUnordered, StreamExt};
use ibc_eureka_solidity_types::ics26::{router::routerInstance, IICS26RouterMsgs::Packet};

use crate::adapter_client::{
    AttestationAdapter,
};
use crate::AttestorError;
use ibc_eureka_solidity_types::msgs::IAttestorMsgs;

pub use config::OpClientConfig;

#[derive(Debug)]
pub struct OpClient {
    client: RootProvider,
    router: routerInstance<RootProvider>,
}

impl OpClient {
    pub fn from_config(config: &OpClientConfig) -> Result<Self, AttestorError> {
        let url = config
            .url
            .parse()
            // Manual map here as the underlying error
            // cannot be imported and `parse` requires
            // type notation
            .map_err(|_| {
                AttestorError::ClientConfigError(format!("url {} could not be parsed", config.url))
            })?;

        let client = RootProvider::<Ethereum>::new_http(url);

        let address = Address::from_hex(&config.router_address)
            .map_err(|e| AttestorError::ClientConfigError(e.to_string()))?;

        let router = routerInstance::new(address, client.clone());

        Ok(Self { client, router })
    }

    async fn get_timestamp_for_block_at_height(&self, height: u64) -> Result<u64, AttestorError> {
        self.client
            .get_block_by_number(BlockNumberOrTag::Number(height))
            .await
            .map_err(|e| AttestorError::ClientError(e.to_string()))?
            .ok_or_else(|| {
                AttestorError::ClientError(format!("no OP block of kind {height} found"))
            })
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

        // Array of 0s means not found
        let is_empty = cmt.iter().max() == Some(&0);
        if is_empty {
            Err(AttestorError::ClientError(format!(
                "commitment path {hashed_path} at height {block_number} not found in OP L2",
            )))
        } else {
            Ok(*cmt)
        }
    }
}

impl AttestationAdapter for OpClient {
    async fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> Result<IAttestorMsgs::StateAttestation, AttestorError> {
        let ts = self.get_timestamp_for_block_at_height(height).await?;
        Ok(IAttestorMsgs::StateAttestation { height, timestamp: ts })
    }
    async fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &[Vec<u8>],
        height: u64,
    ) -> Result<IAttestorMsgs::PacketAttestation, AttestorError> {
        let mut futures = FuturesUnordered::new();

        tracing::debug!("Total optimism packets received: {}", packets.len());

        for p in packets.iter() {
            let packet = Packet::abi_decode(p).map_err(AttestorError::DecodePacket)?;
            let validate_commitment = async move |packet: Packet, height: u64| {
                let commitment_path = packet.commitment_path();
                let hashed = keccak256(&commitment_path);
                let cmt = self
                    .get_historical_packet_commitment(hashed, height)
                    .await?;

                if packet.commitment() != cmt {
                    Err(AttestorError::InvalidCommitment {
                        reason: "requested and received packet commitments do not match".into(),
                    })
                } else {
                    Ok(cmt)
                }
            };
            futures.push(validate_commitment(packet, height));
        }

        let mut validated: Vec<[u8; 32]> = Vec::with_capacity(futures.len());
        while let Some(maybe_cmt) = futures.next().await {
            match maybe_cmt {
                Ok(cmt) => validated.push(cmt),
                Err(e) => return Err(e),
            }
        }

        tracing::debug!("Total optimism packets validated : {}", validated.len());

        let packets: Vec<FixedBytes<32>> = validated
            .into_iter()
            .map(FixedBytes::<32>::from)
            .collect();
        Ok(IAttestorMsgs::PacketAttestation { height, packets })
    }
}
