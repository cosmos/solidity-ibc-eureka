use attestor_packet_membership::Packets;
use std::{fmt::Debug, future::Future};
use thiserror::Error;
use tonic::{Code, Status};

use ibc_eureka_solidity_types::ics26::IICS26RouterMsgs::Packet;

pub trait Signable: Sync + Send {
    fn to_serde_encoded_bytes(&self) -> Vec<u8>;
    fn height(&self) -> u64;
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
    fn to_serde_encoded_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).unwrap()
    }
    fn height(&self) -> u64 {
        self.height
    }
}

impl Signable for UnsignedPacketAttestation {
    fn to_serde_encoded_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(&self.packets).unwrap()
    }
    fn height(&self) -> u64 {
        self.height
    }
}

pub trait Adapter: Sync + Send + 'static {
    fn get_unsigned_state_attestation_at_height(
        &self,
        height: u64,
    ) -> impl Future<Output = Result<UnsignedStateAttestation, AdapterError>> + Send;

    fn get_latest_unsigned_packet_attestation(
        &self,
        packet: &Packets,
    ) -> impl Future<Output = Result<UnsignedPacketAttestation, AdapterError>> + Send;
}

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("Failed to fetch latest finalized block due to {0}")]
    FinalizedBlockError(String),
    #[error("Failed to fetch latest unfinalized block due to {0}")]
    UnfinalizedBlockError(String),
}

impl From<AdapterError> for Status {
    fn from(value: AdapterError) -> Self {
        Status::new(Code::Internal, value.to_string())
    }
}
