//!  Cosmos backwards compatible relayer module.

#![deny(clippy::nursery, clippy::pedantic, warnings, missing_docs)]
#![allow(unused_crate_dependencies)] // Temporary until v1.2 is implemented

use ibc_eureka_utils::rpc::TendermintRpcExt as _;
use ibc_proto_eureka::ibc::lightclients::wasm::v1::ClientState;
use prost::Message;

use crate::listener::cosmos_sdk;

pub enum LightClientVersion {
    V1_2,
    V2,
}

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
                "Failed to get client state of {}: {e}",
                dst_client_id
            ))
        })?;

    let checksum = ClientState::decode(&*client_state.value)
        .map_err(|e| {
            tonic::Status::internal(format!(
                "Failed to decode client state of {}: {e}",
                dst_client_id
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
