use std::{collections::BTreeMap, time::Duration, net::SocketAddr};
use tokio::{time::sleep, net::TcpListener};
use tonic::{transport::Server, Request, Response, Status};
use rand::Rng;
use crate::rpc::{
    attestation_service_server::{AttestationService, AttestationServiceServer},
    AttestationEntry, AttestationsFromHeightRequest, AttestationsFromHeightResponse,
};

#[derive(Debug, Default)]
pub struct MockAttestor {
    // Using BTreeMap to keep heights sorted.
    store: BTreeMap<u64, (Vec<u8>, Vec<u8>)>,
    // To simulate failures
    should_fail: bool,
    // To simulate latency
    delay_ms: u64,
    pub_key: Vec<u8>,
}

impl MockAttestor {
    pub fn new(should_fail: bool, delay_ms: u64) -> Self {
        let mut store = BTreeMap::new();
        // Populate with some data
        for i in 95..=105 {
            // Let's create some forks/disagreements.
            // Attestors that don't fail will agree on height 100, but disagree on 105.
            let height = if i == 105 && !should_fail { 104 } else { i };
            store.insert(height, (vec![height as u8; 12], vec![height as u8; 64]));
        }
        // A higher block that only some attestors will have quorum for.
        if !should_fail {
            store.insert(110, (vec![110; 12], vec![110; 64]));
        }

        let mut pub_key = [0u8; 58];
        rand::rng().fill(&mut pub_key[..]);

        Self {
            store,
            should_fail,
            delay_ms,
            pub_key: pub_key.to_vec(),
        }
    }
}

#[tonic::async_trait]
impl AttestationService for MockAttestor {
    async fn get_attestations_from_height(
        &self,
        request: Request<AttestationsFromHeightRequest>,
    ) -> Result<Response<AttestationsFromHeightResponse>, Status> {
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }

        if self.should_fail {
            return Err(Status::internal("Simulated attestor failure"));
        }

        let min_height = request.into_inner().height;
        let store = self.store.clone();

        let attestations = store
            .range(min_height..)
            .map(|(&height, (state, signature))| AttestationEntry {
                height,
                data: state.clone(),
                signature: signature.clone(),
            })
            .collect();

        Ok(Response::new(AttestationsFromHeightResponse {
            pubkey: self.pub_key.clone(),
            attestations,
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
