use alloy::sol_types::SolValue;

use crate::adapter_client::{
    AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation,
};

use ibc_eureka_utils::rpc::TendermintRpcExt;
use tendermint_rpc::HttpClient;

use attestor_packet_membership::Packets;
use futures::{stream::FuturesUnordered, StreamExt};
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;

use crate::AttestorError;

pub use config::CosmosClientConfig;

mod config;

#[derive(Debug)]
pub struct CosmosClient {
    rpc: HttpClient,
}

impl CosmosClient {
    pub fn from_config(config: &CosmosClientConfig) -> Self {
        Self {
            rpc: HttpClient::from_rpc_url(&config.url),
        }
    }

    async fn get_timestamp_for_block_at_height(&self, height: u64) -> Result<u64, AttestorError> {
        let block = self
            .rpc
            .get_light_block(height.into())
            .await
            .map_err(|e| AttestorError::ClientError(e.to_string()))?;

        let timestamp = block.time().unix_timestamp();

        Ok(timestamp as u64)
    }

    async fn get_historical_packet_commitment(
        &self,
        packet: &Packet,
        height: u64,
    ) -> Result<[u8; 32], AttestorError> {
        let res = self
            .rpc
            .v2_packet_commitment(packet.sourceClient.clone(), packet.sequence, height, false)
            .await
            .map_err(|e| AttestorError::ClientError(e.to_string()))?;

        if res.commitment.len() != 32 {
            return Err(AttestorError::ClientError(format!(
                "commitment length mismatch (got {} bytes, want 32)",
                res.commitment.len()
            )));
        }

        let as_arr: [u8; 32] = res.commitment.try_into().unwrap();

        Ok(as_arr)
    }
}

impl AttestationAdapter for CosmosClient {
    async fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> Result<UnsignedStateAttestation, AttestorError> {
        let timestamp = self.get_timestamp_for_block_at_height(height).await?;

        Ok(UnsignedStateAttestation { height, timestamp })
    }

    async fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &Packets,
        height: u64,
    ) -> Result<UnsignedPacketAttestation, AttestorError> {
        let mut futures = FuturesUnordered::new();

        tracing::debug!(
            "Total cosmos packets received: {}",
            packets.packets().count()
        );

        for p in packets.packets() {
            let packet = Packet::abi_decode(p).map_err(AttestorError::DecodePacket)?;

            // concurrency validate packets against RPC data
            let packet_validator = async move |packet: Packet, height: u64| {
                let commitment = self
                    .get_historical_packet_commitment(&packet, height)
                    .await?;

                if packet.commitment() != commitment {
                    return Err(AttestorError::InvalidCommitment {
                        reason: "requested and received packet commitments do not match".into(),
                    });
                }

                Ok(commitment)
            };

            futures.push(packet_validator(packet, height));
        }

        let mut validated_commitments = Vec::with_capacity(futures.len());
        while let Some(maybe_cmt) = futures.next().await {
            match maybe_cmt {
                Ok(cmt) => validated_commitments.push(cmt),
                Err(e) => return Err(e),
            }
        }

        tracing::debug!(
            "Total cosmos packets validated: {}",
            validated_commitments.len()
        );

        Ok(UnsignedPacketAttestation {
            height,
            packets: validated_commitments,
        })
    }
}
