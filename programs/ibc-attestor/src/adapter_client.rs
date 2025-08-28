use alloy_sol_types::SolType;
use std::future::Future;
use alloy_sol_types::SolType;

use crate::AttestorError;
use ibc_eureka_solidity_types::msgs::IAttestorMsgs;

pub trait Signable: Sync + Send {
    fn to_abi_encoded_bytes(&self) -> Result<Vec<u8>, alloy_sol_types::Error>;
    fn height(&self) -> u64;
    fn timestamp(&self) -> Option<u64>;
}

// Removed UnsignedPacketAttestation in favor of using IAttestorMsgs::PacketAttestation directly

impl Signable for IAttestorMsgs::StateAttestation {
    fn to_abi_encoded_bytes(&self) -> Result<Vec<u8>, alloy_sol_types::Error> {
        Ok(IAttestorMsgs::StateAttestation::abi_encode(self))
    }
    fn height(&self) -> u64 {
        self.height
    }
    fn timestamp(&self) -> Option<u64> {
        Some(self.timestamp)
    }
}

impl Signable for IAttestorMsgs::PacketAttestation {
    fn to_abi_encoded_bytes(&self) -> Result<Vec<u8>, alloy_sol_types::Error> {
        Ok(IAttestorMsgs::PacketAttestation::abi_encode(self))
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
    ) -> impl Future<Output = Result<IAttestorMsgs::StateAttestation, AttestorError>> + Send;

    fn get_unsigned_packet_attestation_at_height(
        &self,
        packets: &[Vec<u8>],
        height: u64,
    ) -> impl Future<Output = Result<IAttestorMsgs::PacketAttestation, AttestorError>> + Send;
}
