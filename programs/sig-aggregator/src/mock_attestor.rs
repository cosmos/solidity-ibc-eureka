use crate::{
    attestor_data::{PUBKEY_BYTE_LENGTH, SIGNATURE_BYTE_LENGTH, STATE_BYTE_LENGTH},
    rpc::{
        attestation_service_server::{AttestationService, AttestationServiceServer},
        Attestation,
        PacketAttestationRequest, PacketAttestationResponse,
        StateAttestationRequest, StateAttestationResponse,
    },
};
use rand::Rng;
use std::{collections::BTreeMap, net::SocketAddr, time::Duration};
use tokio::{net::TcpListener, time::sleep};
use tonic::{transport::Server, Request, Response, Status};

#[derive(Debug, Default)]
pub struct MockAttestor {
    // Using BTreeMap to keep heights sorted.
    store: Attestation,
    // To simulate failures
    should_fail: bool,
    // To simulate latency
    delay_ms: u64,
}

impl MockAttestor {
    pub fn new(should_fail: bool, delay_ms: u64) -> Self {
        let mut store = Attestation { 
            height: 110, 
            attested_data: vec![110; STATE_BYTE_LENGTH], 
            signature: vec![110; SIGNATURE_BYTE_LENGTH], 
            public_key: vec![110; PUBKEY_BYTE_LENGTH] 
        };

        if should_fail {
            store.height = 105;
            store.attested_data = vec![105; STATE_BYTE_LENGTH];
            store.signature = vec![105; SIGNATURE_BYTE_LENGTH];
            store.public_key = vec![105; PUBKEY_BYTE_LENGTH];
        }

        Self {
            store,
            should_fail,
            delay_ms,
        }
    }
}

#[tonic::async_trait]
impl AttestationService for MockAttestor {
    async fn packet_attestation(
        &self,
        request: Request<PacketAttestationRequest>,
    ) -> Result<Response<PacketAttestationResponse>, Status> {
        todo!()
    }

    async fn state_attestation(
        &self,
        request: Request<StateAttestationRequest>,
    ) -> Result<Response<StateAttestationResponse>, Status> {
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }

        if self.should_fail {
            return Err(Status::internal("Simulated attestor failure"));
        }

        let min_height = request.into_inner().height;
        let store = self.store.clone();

        let attestation = store
            .max_by_key(|(&height, _)| height)
            .map(|(&height, (state, signature))| Attestation {
                height,
                attested_data: state.clone(),
                signature: signature.clone(),
                public_key: self.pub_key.clone(),
            });

        Ok(Response::new(StateAttestationResponse {
            attestation,
        }))
    }
}

impl MockAttestor {
    pub fn get_pubkey(&self) -> Vec<u8> {
        self.pub_key.clone()
    }
}

pub async fn run_attestor_server(
    addr: String,
    should_fail: bool,
    delay_ms: u64,
) -> anyhow::Result<()> {
    let addr = addr.parse()?;
    let attestor = MockAttestor::new(should_fail, delay_ms);

    tracing::info!("Attestor listening on {}", addr);

    Server::builder()
        .add_service(AttestationServiceServer::new(attestor))
        .serve(addr)
        .await?;

    Ok(())
}

// Helper to spin up a mock attestor server on a random available port.
// Returns the address it's listening on.
pub async fn setup_attestor_server(
    should_fail: bool,
    delay_ms: u64,
) -> anyhow::Result<(SocketAddr, Vec<u8>)> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let attestor = MockAttestor::new(should_fail, delay_ms);
    let pubkey = attestor.get_pubkey();

    tokio::spawn(async move {
        Server::builder()
            .add_service(AttestationServiceServer::new(attestor))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
    });

    Ok((addr, pubkey))
}
