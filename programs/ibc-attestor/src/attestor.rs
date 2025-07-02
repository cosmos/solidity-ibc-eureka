use std::{future::Future, time::Duration};

use indexmap::IndexMap;

use crate::{
    adapter_client::{Adapter, AdapterError, Signable},
    attestation::Attestation,
    height_store::AttestationStore,
    signer::Signer,
};

pub struct AttestorService<A: Adapter> {
    adapter: A,
    signer: Signer,
}

pub trait Attestor: Send + Sync + 'static {
    fn update_frequency(&self) -> Duration;

    fn update_attestation_store(
        &self,
        store: &mut AttestationStore,
    ) -> impl Future<Output = ()> + Send;

    fn attestations_from_height(
        &self,
        height: u64,
        store: &AttestationStore,
    ) -> IndexMap<u64, Attestation>;
}

impl<A> AttestorService<A>
where
    A: Adapter,
{
    pub fn new(adapter: A, signer: Signer) -> Self {
        Self { adapter, signer }
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

    async fn update_attestation_store(&self, store: &mut AttestationStore) {
        let to_sign = self.get_latest_unfinalized_signable().await.unwrap();
        tracing::info!("adding new height: {:#?}", to_sign);
        let store_at_height = to_sign.height();
        let signed = self.signer.sign(to_sign);
        store.push(store_at_height, signed);
    }

    fn attestations_from_height(
        &self,
        height: u64,
        store: &AttestationStore,
    ) -> IndexMap<u64, Attestation> {
        let mut heights = IndexMap::new();
        for (h, v) in store.range_from(height) {
            heights.insert(h.clone(), v.clone());
        }
        heights
    }
}
