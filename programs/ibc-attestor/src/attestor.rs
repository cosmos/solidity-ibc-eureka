use std::{
    future::Future,
    sync::{Arc, Mutex},
    time::Duration,
};

use tonic::{Response, Status};

use crate::{
    adapter_client::{Adapter, AdapterError, Signable},
    api::{
        attestation_service_server::AttestationService, AttestationEntry,
        AttestationsFromHeightRequest, AttestationsFromHeightResponse,
    },
    attestation::Attestation,
    attestation_store::AttestationStore,
    signer::Signer,
};

pub struct AttestorService<A: Adapter> {
    adapter: A,
    signer: Signer,
    // Interior mutability to allow Arc
    // of service
    store: Arc<Mutex<AttestationStore>>,
}

pub trait Attestor: Send + Sync + 'static {
    fn update_frequency(&self) -> Duration;

    fn update_attestation_store(&self) -> impl Future<Output = ()> + Send;

    fn attestations_from_height(&self, height: u64) -> Vec<(u64, Attestation)>;
}

impl<A> AttestorService<A>
where
    A: Adapter,
{
    pub fn new(adapter: A, signer: Signer, store: AttestationStore) -> Self {
        Self {
            adapter,
            signer,
            store: Arc::new(Mutex::new(store)),
        }
    }

    async fn get_latest_finalized_signable<'a>(
        &'a self,
    ) -> Result<impl Signable + 'a, AdapterError> {
        self.adapter.get_latest_finalized_block().await
    }

    async fn get_latest_unfinalized_signable<'a>(
        &'a self,
    ) -> Result<impl Signable + 'a, AdapterError> {
        self.adapter.get_latest_unfinalized_block().await
    }
}

impl<A> Attestor for AttestorService<A>
where
    A: Adapter,
{
    fn update_frequency(&self) -> Duration {
        Duration::from_millis(10)
    }

    async fn update_attestation_store(&self) {
        let to_sign = self.get_latest_unfinalized_signable().await.unwrap();
        tracing::debug!("adding new height: {:#?}", to_sign);
        let store_at_height = to_sign.height();
        let signed = self.signer.sign(to_sign);

        let mut store = self.store.lock().unwrap();
        store.push(store_at_height, signed);
    }

    /// Returns a an [IndexMap] that contains all attestations
    /// in insertion order from a given `height`
    fn attestations_from_height(&self, height: u64) -> Vec<(u64, Attestation)> {
        let store = self.store.lock().unwrap();
        store.range_from(height).cloned().collect()
    }
}

#[tonic::async_trait]
impl<A> AttestationService for Arc<AttestorService<A>>
where
    A: Adapter,
{
    async fn get_attestations_from_height(
        self: Arc<Self>,
        request: tonic::Request<AttestationsFromHeightRequest>,
    ) -> Result<Response<AttestationsFromHeightResponse>, Status> {
        let atts = self.attestations_from_height(request.get_ref().height);
        let as_messages: Vec<_> = atts
            .into_iter()
            .map(|(h, att)| AttestationEntry {
                height: h,
                data: att.data,
                signature: att.signature.to_vec(),
            })
            .collect();

        let message = AttestationsFromHeightResponse {
            pubkey: [0; 58].to_vec(),
            attestations: as_messages,
        };

        Ok(message.into())
    }
}
