use crate::adapter_client::{Adapter, AdapterError, Signable};

struct Attestor<A: Adapter> {
    adapter: A,
}

impl<A> Attestor<A>
where
    A: Adapter,
{
    pub fn new(adapter: A) -> Self {
        Self { adapter }
    }

    pub async fn get_latest_finalized_signable<'a>(
        &'a self,
    ) -> Result<impl Signable + 'a, AdapterError> {
        self.adapter.get_latest_finalized_block().await
    }

    pub async fn get_latest_unfinalized_signable<'a>(
        &'a self,
    ) -> Result<impl Signable + 'a, AdapterError> {
        self.adapter.get_latest_unfinalized_block().await
    }
}
