use indexmap::IndexMap;

use crate::{
    adapter_client::{Adapter, AdapterError, Signable},
    attestation::Attestation,
    height_store::HeightStore,
    signer::Signer,
};

pub struct AttestorConfig {
    pub max_entries: u16,
}

pub struct Attestor<A: Adapter> {
    adapter: A,
    height_store: HeightStore,
    signer: Signer,
}

impl<A> Attestor<A>
where
    A: Adapter,
{
    pub fn new(adapter: A, height_store: HeightStore, signer: Signer) -> Self {
        Self {
            adapter,
            height_store,
            signer,
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

    pub async fn update_height_store(&mut self) {
        let to_sign = self.get_latest_unfinalized_signable().await.unwrap();
        let store_at_height = to_sign.height();
        let signed = self.signer.sign(to_sign);
        self.height_store.push(store_at_height, signed);
    }

    pub fn heights_from(&self, height: u64) -> IndexMap<u64, Attestation> {
        let mut heights = IndexMap::new();
        for (h, v) in self.height_store.range_from(height) {
            heights.insert(h.clone(), v.clone());
        }
        heights
    }
}
