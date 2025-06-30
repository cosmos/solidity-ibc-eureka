use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::sleep;
use tonic::{transport::Server, Request, Response, Status};

use crate::rpc::{
    attestor_server::{Attestor, AttestorServer},
    Attestation, AttestationsResponse, QueryRequest,
};

// A mock signature is just a height repeated 4 times inside a 32-byte array.
// Which represent a digest, i.e. serialized chain header.
fn mock_signature(height: u64) -> Vec<u8> {
    let mut sig = [0u8; 32];
    let height_bytes = height.to_be_bytes();
    for i in 0..4 {
        sig[i * 8..(i + 1) * 8].copy_from_slice(&height_bytes);
    }
    sig.to_vec()
}

#[derive(Debug, Default)]
pub struct MockAttestor {
    // Using BTreeMap to keep heights sorted.
    store: Arc<Mutex<BTreeMap<u64, Vec<u8>>>>,
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
            store.insert(height, mock_signature(height));
        }
        // A higher block that only some attestors will have quorum for.
        if !should_fail {
            store.insert(110, mock_signature(110));
        }

        Self {
            store: Arc::new(Mutex::new(store)),
            should_fail,
            delay_ms,
            pub_key: [0u8; 65].to_vec(),
        }
    }
}

#[tonic::async_trait]
impl Attestor for MockAttestor {
    async fn query_attestations(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<AttestationsResponse>, Status> {
        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }

        if self.should_fail {
            return Err(Status::internal("Simulated attestor failure"));
        }

        let min_height = request.into_inner().min_height;
        let store = self.store.lock().unwrap();

        let attestations = store
            .range(min_height..)
            .map(|(&height, signature)| Attestation {
                height,
                signature: signature.clone(),
                pubkey: self.pub_key.clone(),
            })
            .collect();

        Ok(Response::new(AttestationsResponse { attestations }))
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
        .add_service(AttestorServer::new(attestor))
        .serve(addr)
        .await?;

    Ok(())
}
