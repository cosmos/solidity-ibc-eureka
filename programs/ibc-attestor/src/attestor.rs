use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use tonic::{Response, Status};

use crate::{
    adapter_client::{Adapter, AdapterError, Signable},
    api::{
        attestation_service_server::AttestationService, Attestation, PacketAttestationRequest,
        PacketAttestationResponse, StateAttestationRequest, StateAttestationResponse,
    },
    signer::Signer,
};

/// Provides read access to and fetches and signs new data for
/// the attestation store.
///
/// The [AttestorService] is composed of three parts:
/// - A generic [Adapter] client that fetches [Signable] data
/// - A concrete [Signer] that uses the `sepc256k1` aglo for
///   cryptographic signatures
/// - An internally mutable instance of the [AttestationStore]
///
/// The relationship between these components is as follows:
/// - The service when run in a loop should update its store
///   using [Attestor::update_attestation_store]. The frequency of these updates
///   should be determined by [Attestor::update_frequency].
/// - Once raw data has been retrieved the service uses the [Signer]
///   to make the data cryptographically verifiable by a given light
///   client in the future.
/// - The signed data is stored in the [AttestationStore] and made
///   accessible via the [Attestor::attestations_from_height] method.
///
/// These methods use internal types before converting them into
/// RPC generated types in the [AttestationService] trait implementation.
pub struct AttestorService<A: Adapter> {
    adapter: A,
    signer: Signer,
}

pub trait Attestor: Send + Sync + 'static {
    fn state_attestation(&self, height: u64) -> Attestation;

    fn packet_attestation(&self, height: u64) -> Attestation;
}

impl<A> AttestorService<A>
where
    A: Adapter,
{
    pub fn new(adapter: A, signer: Signer) -> Self {
        Self { adapter, signer }
    }

    async fn _get_latest_finalized_signable<'a>(
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
    fn state_attestation(&self, _height: u64) -> Attestation {
        todo!()
    }

    fn packet_attestation(&self, _height: u64) -> Attestation {
        todo!()
    }
}

/// *Note*: This RPC auto-generated trait uses the [Arc<Self>] option to
/// make it possible to share the [AttestorService] across threads.
#[tonic::async_trait]
impl<A> AttestationService for Arc<AttestorService<A>>
where
    A: Adapter,
{
    async fn state_attestation(
        self: Arc<Self>,
        _request: tonic::Request<StateAttestationRequest>,
    ) -> Result<Response<StateAttestationResponse>, Status> {
        todo!()
    }

    async fn packet_attestation(
        self: Arc<Self>,
        _request: tonic::Request<PacketAttestationRequest>,
    ) -> Result<Response<PacketAttestationResponse>, Status> {
        todo!()
    }
}
