//!  Cosmos backwards compatible relayer module.

#![deny(clippy::nursery, clippy::pedantic, warnings, missing_docs)]
#![allow(unused_crate_dependencies)] // Temporary until v1.2 is implemented

use ibc_eureka_utils::rpc::TendermintRpcExt as _;
use ibc_proto_eureka::ibc::lightclients::wasm::v1::ClientState;
use prost::Message;

use crate::listener::cosmos_sdk;

// Which IBC version supports counterparty chain lightclient deployed on Cosmos
pub enum LightClientVersion {
    /// Version 1
    V1_2,
    /// Version 2
    V2,
}

/// Determines the Cosmos light client version by checking the client state checksum.
///
/// # Arguments
/// * `tm_listener` - Chain listener for querying the Cosmos chain
/// * `dst_client_id` - Client ID to check the version for
/// * `v1_2_checksum` - Expected checksum bytes for v1.2 clients
///
/// # Returns
/// * `Ok(LightClientVersion::V1_2)` - If checksum matches v1.2
/// * `Ok(LightClientVersion::V2)` - If checksum doesn't match (assumes v2)
/// * `Err` - If client state query or decoding fails
///
/// # Errors
/// - Failed to query client state from chain
/// - Failed to decode client state protobuf
pub async fn cosmos_light_client_version(
    tm_listener: &cosmos_sdk::ChainListener,
    dst_client_id: String,
    v1_2_checksum: &[u8],
) -> Result<LightClientVersion, tonic::Status> {
    let client_state = tm_listener
        .client()
        .client_state(dst_client_id.clone())
        .await
        .map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to get client state of {dst_client_id}: {e}",
            ))
        })?;

    let checksum = ClientState::decode(&*client_state.value)
        .map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to decode client state of {dst_client_id}: {e}",
            ))
        })?
        .checksum;

    // TODO: When v1.2 is available, check client state checksum and use appropriate service
    // For now, just forward to v2 service
    if checksum == v1_2_checksum {
        Ok(LightClientVersion::V1_2)
    } else {
        Ok(LightClientVersion::V2)
    }
}
