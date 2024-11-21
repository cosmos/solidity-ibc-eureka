//! The `ChainSubmitter` submits txs to [`EthEureka`] based on events from [`CosmosSdk`].

use alloy::{
    primitives::{Address, TxHash},
    providers::Provider,
    transports::Transport,
};
use anyhow::Result;
use ibc_eureka_solidity_types::ics26::router::routerInstance;
use tendermint_rpc::HttpClient;

use crate::{
    chain::{CosmosSdk, EthEureka},
    events::EurekaEvent,
};

use super::r#trait::ChainSubmitterService;

/// The `ChainSubmitter` submits txs to [`EthEureka`] based on events from [`CosmosSdk`].
#[allow(dead_code)]
pub struct ChainSubmitter<T: Transport + Clone, P: Provider<T>> {
    /// The IBC Eureka router instance.
    ics26_router: routerInstance<T, P>,
    /// The HTTP client for the Cosmos SDK.
    tm_client: HttpClient,
}

impl<T: Transport + Clone, P: Provider<T>> ChainSubmitter<T, P> {
    /// Create a new `ChainListenerService` instance.
    pub const fn new(ics26_address: Address, provider: P, tm_client: HttpClient) -> Self {
        Self {
            ics26_router: routerInstance::new(ics26_address, provider),
            tm_client,
        }
    }
}

#[async_trait::async_trait]
impl<T, P> ChainSubmitterService<EthEureka, CosmosSdk> for ChainSubmitter<T, P>
where
    T: Transport + Clone,
    P: Provider<T>,
{
    async fn submit_events(
        &self,
        src_events: Vec<EurekaEvent>,
        dest_events: Vec<EurekaEvent>,
    ) -> Result<TxHash> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let _timeout_events = src_events.into_iter().filter(
            |e| matches!(e, EurekaEvent::SendPacket(se) if now >= se.packet.timeoutTimestamp),
        );

        let _recv_ack_events = dest_events.into_iter().filter(|e| match e {
            EurekaEvent::SendPacket(se) => se.packet.timeoutTimestamp > now,
            EurekaEvent::WriteAcknowledgement(_) => true,
            _ => false,
        });

        let _ibc_paths = _timeout_events
            .map(|e| match e {
                EurekaEvent::SendPacket(se) => se.packet.receipt_commitment_path(),
                _ => unreachable!(),
            })
            .chain(_recv_ack_events.map(|e| match e {
                EurekaEvent::SendPacket(se) => se.packet.commitment_path(),
                EurekaEvent::WriteAcknowledgement(we) => we.packet.ack_commitment_path(),
                _ => unreachable!(),
            }))
            .map(|path| vec![b"ibc".into(), path]);

        todo!()
    }
}
