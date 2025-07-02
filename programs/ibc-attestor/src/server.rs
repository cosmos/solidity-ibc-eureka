use crate::{attestation_store::AttestationStore, attestor::Attestor};

pub struct Server;

impl Server {
    pub async fn start(
        &self,
        service: impl Attestor,
        mut store: AttestationStore,
    ) -> Result<(), anyhow::Error> {
        let mut attestor_ticker = tokio::time::interval(service.update_frequency());

        loop {
            tokio::select! {
                _ = attestor_ticker.tick() => {
                    tracing::info!("Updating attestor heights");
                    service.update_attestation_store(&mut store).await;
                }
            }
        }
    }
}
