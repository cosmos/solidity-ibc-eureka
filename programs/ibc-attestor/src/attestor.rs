use std::sync::Arc;

use attestor_packet_membership::Packets;
use tonic::{Response, Status};

use crate::{
    adapter_client::{Adapter, AdapterError},
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

impl<A> AttestorService<A>
where
    A: Adapter,
{
    pub fn new(adapter: A, signer: Signer) -> Self {
        Self { adapter, signer }
    }

    /// Forwards to the [Adapter] and uses the [Signer] to
    /// sign the result.
    pub async fn get_latest_state_attestation(
        &self,
        height: u64,
    ) -> Result<Attestation, AdapterError> {
        let unsigned = self
            .adapter
            .get_unsigned_state_attestation_at_height(height)
            .await?;
        let signed = self.signer.sign(unsigned);
        Ok(signed)
    }

    /// Forwards to the [Adapter] and uses the [Signer] to
    /// sign the result.
    pub async fn get_latest_packet_attestation(
        &self,
        packets: &Packets,
        height: u64,
    ) -> Result<Attestation, AdapterError> {
        let unsigned = self
            .adapter
            .get_unsigned_packet_attestation_at_height(&packets, height)
            .await?;
        let signed = self.signer.sign(unsigned);
        Ok(signed)
    }
}

#[tonic::async_trait]
impl<A> AttestationService for Arc<AttestorService<A>>
where
    A: Adapter,
{
    async fn state_attestation(
        &self,
        request: tonic::Request<StateAttestationRequest>,
    ) -> Result<Response<StateAttestationResponse>, Status> {
        let att = self
            .get_latest_state_attestation(request.into_inner().height)
            .await?;
        Ok(StateAttestationResponse {
            attestation: Some(att),
        }
        .into())
    }

    async fn packet_attestation(
        &self,
        request: tonic::Request<PacketAttestationRequest>,
    ) -> Result<Response<PacketAttestationResponse>, Status> {
        let request_inner = request.into_inner();
        let packets = Packets::new(request_inner.packets);
        let att = self.get_latest_packet_attestation(&packets, request_inner.height).await?;
        Ok(PacketAttestationResponse {
            attestation: Some(att),
        }
        .into())
    }
}
