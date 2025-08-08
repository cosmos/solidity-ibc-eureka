use attestor_packet_membership::Packets;
use std::future::Future;

use crate::AttestorError;

pub trait Signable: Sync + Send {
    fn to_serde_encoded_bytes(&self) -> Result<Vec<u8>, serde_json::Error>;
    fn height(&self) -> u64;
    fn timestamp(&self) -> Option<u64>;
}

pub struct UnsignedPacketAttestation {
    pub height: u64,
    pub packets: Vec<[u8; 32]>,
}

#[derive(serde::Serialize)]
pub struct UnsignedStateAttestation {
    pub height: u64,
    pub timestamp: u64,
}

impl Signable for UnsignedStateAttestation {
    fn to_serde_encoded_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }
    fn height(&self) -> u64 {
        self.height
    }
    fn timestamp(&self) -> Option<u64> {
        Some(self.timestamp)
    }
}

impl Signable for UnsignedPacketAttestation {
    fn to_serde_encoded_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(&self.packets)
    }
    fn height(&self) -> u64 {
        self.height
    }
    fn timestamp(&self) -> Option<u64> {
        None
    }
}

pub trait AttestationAdapter: Sync + Send + 'static {
    fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> impl Future<Output = Result<UnsignedStateAttestation, AttestorError>> + Send;

    fn get_unsigned_packet_attestation_at_height(
        &self,
        packet: &Packets,
        height: u64,
    ) -> impl Future<Output = Result<UnsignedPacketAttestation, AttestorError>> + Send;
}
