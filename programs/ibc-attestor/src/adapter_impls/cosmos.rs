use alloy_primitives::keccak256;
use alloy_sol_types::SolType;
use tendermint::block::Height;

use crate::adapter_client::AttestationAdapter;

use ibc_eureka_utils::rpc::TendermintRpcExt;
use tendermint_rpc::{Client, HttpClient};

use futures::{stream::FuturesUnordered, StreamExt};
use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;
use ibc_eureka_solidity_types::msgs::IAttestorMsgs;

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
        let h = TryInto::<Height>::try_into(height)
            .map_err(|e| AttestorError::ClientError(format!("Invalid height {}: {}", height, e)))?;

        let block = self.rpc.commit(h).await.map_err(|e| {
            tracing::error!("Failed to get block commit at height {}: {}", height, e);
            AttestorError::ClientError(format!(
                "Failed to retrieve block at height {}: {}",
                height, e
            ))
        })?;

        let ts = block.signed_header.header.time.unix_timestamp();

        Ok(ts as u64)
    }

    async fn get_historical_packet_commitment(
        &self,
        packet: &Packet,
        height: u64,
    ) -> Result<[u8; 32], AttestorError> {
        let client_id = packet.sourceClient.clone();

        let res = self
            .rpc
            .v2_packet_commitment(client_id, packet.sequence, height, false)
            .await
            .map_err(|e| AttestorError::ClientError(e.to_string()))?;

        if res.commitment.is_empty() {
            return Err(AttestorError::ClientError("empty commitment".to_string()));
        }

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
    ) -> Result<IAttestorMsgs::StateAttestation, AttestorError> {
        let timestamp = self.get_timestamp_for_block_at_height(height).await?;

        Ok(IAttestorMsgs::StateAttestation { height, timestamp })
    }

    async fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &[Vec<u8>],
        height: u64,
    ) -> Result<IAttestorMsgs::PacketAttestation, AttestorError> {
        let mut futures = FuturesUnordered::new();

        tracing::debug!("Total cosmos packets received: {}", packets.len());

        for p in packets {
            let packet = Packet::abi_decode(p).map_err(AttestorError::DecodePacket)?;

            // concurrency validate packets against RPC data
            let packet_validator = async move |packet: Packet, height: u64| {
                let expected_commitment = packet.commitment();

                let commitment = match self.get_historical_packet_commitment(&packet, height).await
                {
                    Ok(commitment) => commitment,
                    Err(err) => {
                        tracing::error!(
                            "Commitment failed: {:?}, error: {}; expected 0x{}",
                            packet,
                            err,
                            hex::encode(&expected_commitment),
                        );
                        return Err(err);
                    }
                };

                if expected_commitment != commitment {
                    return Err(AttestorError::InvalidCommitment {
                        reason: format!(
                            "Commitment mismatch: request carried 0x{}, but rpc returned 0x{}",
                            hex::encode(expected_commitment),
                            hex::encode(commitment),
                        ),
                    });
                }

                Ok(IAttestorMsgs::PacketCompact {
                    path: keccak256(packet.commitment_path()),
                    commitment: commitment.into(),
                })
            };

            futures.push(packet_validator(packet, height));
        }

        let mut validated = Vec::with_capacity(futures.len());

        while let Some(maybe_cmt) = futures.next().await {
            match maybe_cmt {
                Ok(cmt) => validated.push(cmt),
                Err(e) => return Err(e),
            }
        }

        Ok(IAttestorMsgs::PacketAttestation {
            height,
            packets: validated,
        })
    }
}
