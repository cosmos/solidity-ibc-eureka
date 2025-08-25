mod config;

pub use config::CosmosClientConfig;

use crate::adapter_client::{
    AttestationAdapter, UnsignedPacketAttestation, UnsignedStateAttestation,
};

use ibc_eureka_utils::rpc::TendermintRpcExt;
use tendermint_rpc::HttpClient;

use attestor_packet_membership::Packets;

use crate::AttestorError;

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
        _hashed_path: [u8; 32],
        _block_number: u64,
    ) -> Result<[u8; 32], AttestorError> {
        // todo implement grpc call for getCommitment
        todo!()
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
        _packets: &Packets,
        _height: u64,
    ) -> Result<UnsignedPacketAttestation, AttestorError> {
        todo!()
    }
}
