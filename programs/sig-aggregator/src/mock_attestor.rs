use crate::{
    attestor_data::{PUBKEY_BYTE_LENGTH, SIGNATURE_BYTE_LENGTH, STATE_BYTE_LENGTH},
    rpc::{
        attestation_service_server::{AttestationService, AttestationServiceServer},
        Attestation, PacketAttestationRequest, PacketAttestationResponse, StateAttestationRequest,
        StateAttestationResponse,
    },
};
use attestor_packet_membership::Packets;
use std::{net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, time::sleep};
use tonic::{transport::Server, Request, Response, Status};

#[derive(Debug, Default)]
pub struct MockAttestor {
    pub_key: Vec<u8>,
    // To simulate malicious attestor
    is_malicious: bool,
    // To simulate latency
    delay_ms: u64,
}

impl MockAttestor {
    pub fn new(malicious: bool, delay_ms: u64, id: u8) -> Self {
        Self {
            pub_key: vec![id; PUBKEY_BYTE_LENGTH],
            is_malicious: malicious,
            delay_ms,
        }
    }

    pub fn get_state_attestation(&self, height: u64) -> Attestation {
        let value = if self.is_malicious { 0 } else { 42 };
        Attestation {
            height,
            attested_data: vec![value; STATE_BYTE_LENGTH],
            signature: vec![value; SIGNATURE_BYTE_LENGTH],
            public_key: self.pub_key.clone(),
        }
    }

    // For this mock attestor, we can ignore the packet data. We can return the same height as the
    // request.
    pub fn get_packet_attestation(&self, height: u64, _packet: Packets) -> Attestation {
        let value = if self.is_malicious { 0 } else { 42 };
        Attestation {
            height,
            attested_data: vec![value; STATE_BYTE_LENGTH],
            signature: vec![value; SIGNATURE_BYTE_LENGTH],
            public_key: self.pub_key.clone(),
        }
    }
}

#[tonic::async_trait]
impl AttestationService for MockAttestor {
    async fn packet_attestation(
        &self,
        request: Request<PacketAttestationRequest>,
    ) -> Result<Response<PacketAttestationResponse>, Status> {
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }

        let request = request.into_inner();
        if request.packets.is_empty() {
            return Err(Status::invalid_argument("Packets cannot be empty"));
        }

        let attestation = self.get_packet_attestation(request.height, request.packets);

        Ok(Response::new(PacketAttestationResponse {
            attestation: Some(attestation),
        }))
    }

    async fn state_attestation(
        &self,
        request: Request<StateAttestationRequest>,
    ) -> Result<Response<StateAttestationResponse>, Status> {
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }

        let attestation = self.get_state_attestation(request.into_inner().height);
        Ok(Response::new(StateAttestationResponse {
            attestation: Some(attestation),
        }))
    }
}

// Helper to spin up a mock attestor server on a random available port.
// Returns the address it's listening on.
pub async fn setup_attestor_server(
    malicious: bool,
    delay_ms: u64,
    id: u8,
) -> anyhow::Result<(SocketAddr, Vec<u8>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let attestor = MockAttestor::new(malicious, delay_ms, id);
    let pubkey = attestor.pub_key.clone();

    tokio::spawn(async move {
        Server::builder()
            .add_service(AttestationServiceServer::new(attestor))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
    });

    Ok((addr, pubkey))
}
