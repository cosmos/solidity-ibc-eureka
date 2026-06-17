//! Ethereum to Cosmos proof API module for the v1.3.0 Ethereum wasm client.

#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

use ibc_eureka_relayer_core_v1_3::{
    api::{self as api_v1_3, relayer_service_server::RelayerService as RelayerServiceV1_3},
    modules::RelayerModule as RelayerModuleV1_3,
};
use ibc_eureka_relayer_eth_to_cosmos_v1_3::EthToCosmosRelayerModule as EthToCosmosRelayerModuleV1_3;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use ibc_proto_eureka::ibc::lightclients::wasm::v1::ClientState;
use proof_api_core::{
    api::{self, proof_api_service_server::ProofApiService},
    modules::ProofApiModule,
};
use proof_api_lib::listener::cosmos_sdk;
use prost::Message;
use serde_json::Value;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

/// The checksum for the v1.3 Ethereum wasm client.
const V1_3_CHECKSUM: &[u8] = &[
    175, 132, 204, 204, 163, 231, 70, 217, 196, 234, 152, 12, 109, 27, 69, 17, 222, 15, 169, 98,
    237, 80, 3, 222, 232, 203, 68, 237, 161, 14, 69, 104,
];

/// The hex checksum for the v1.3 Ethereum wasm client.
const V1_3_CHECKSUM_HEX: &str = "af84cccca3e746d9c4ea980c6d1b4511de0fa962ed5003dee8cb44eda10e4568";

/// The key for the checksum hex in the parameters map.
const CHECKSUM_HEX: &str = "checksum_hex";

/// The Ethereum to Cosmos v1.3.0 proof API module.
#[derive(Clone, Copy, Debug)]
pub struct EthToCosmosV1_3_0ProofApiModule;

/// The proof API service from Ethereum to Cosmos for the v1.3.0 Ethereum wasm client.
struct EthToCosmosV1_3_0ProofApiModuleService {
    tm_listener: cosmos_sdk::ChainListener,
    v1_3_service: Box<dyn RelayerServiceV1_3>,
}

impl EthToCosmosV1_3_0ProofApiModuleService {
    fn new(tm_rpc_url: &str, v1_3_service: Box<dyn RelayerServiceV1_3>) -> Self {
        let tm_client = HttpClient::from_rpc_url(tm_rpc_url);
        let tm_listener = cosmos_sdk::ChainListener::new(tm_client);
        Self {
            tm_listener,
            v1_3_service,
        }
    }

    async fn client_checksum(&self, dst_client_id: &str) -> Result<Vec<u8>, tonic::Status> {
        let client_state = self
            .tm_listener
            .client()
            .client_state(dst_client_id.to_string())
            .await
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to get client state of {dst_client_id}: {e}"
                ))
            })?;

        Ok(ClientState::decode(&*client_state.value)
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to decode client state of {dst_client_id}: {e}"
                ))
            })?
            .checksum)
    }

    fn ensure_v1_3_checksum(checksum: &[u8]) -> Result<(), tonic::Status> {
        if checksum == V1_3_CHECKSUM {
            Ok(())
        } else {
            Err(tonic::Status::failed_precondition(format!(
                "eth_to_cosmos_v1.3.0 only supports Ethereum wasm client checksum {V1_3_CHECKSUM_HEX}, got {}",
                hex::encode(checksum)
            )))
        }
    }
}

#[tonic::async_trait]
impl ProofApiService for EthToCosmosV1_3_0ProofApiModuleService {
    #[tracing::instrument(skip_all, err(Debug))]
    async fn info(
        &self,
        request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        let inner = request.into_inner();
        let resp = self
            .v1_3_service
            .info(Request::new(api_v1_3::InfoRequest {
                src_chain: inner.src_chain,
                dst_chain: inner.dst_chain,
            }))
            .await?
            .into_inner();

        Ok(Response::new(api::InfoResponse {
            target_chain: resp.target_chain.map(|chain| api::Chain {
                chain_id: chain.chain_id,
                ibc_version: chain.ibc_version,
                ibc_contract: chain.ibc_contract,
            }),
            source_chain: resp.source_chain.map(|chain| api::Chain {
                chain_id: chain.chain_id,
                ibc_version: chain.ibc_version,
                ibc_contract: chain.ibc_contract,
            }),
            metadata: resp.metadata,
        }))
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        let req = request.get_ref();
        let checksum = self.client_checksum(&req.dst_client_id).await?;
        Self::ensure_v1_3_checksum(&checksum)?;

        tracing::info!("Using Ethereum to Cosmos v1.3.0 relay_by_tx");
        let inner = request.into_inner();
        let resp = self
            .v1_3_service
            .relay_by_tx(Request::new(api_v1_3::RelayByTxRequest {
                src_chain: inner.src_chain,
                dst_chain: inner.dst_chain,
                source_tx_ids: inner.source_tx_ids,
                timeout_tx_ids: inner.timeout_tx_ids,
                src_client_id: inner.src_client_id,
                dst_client_id: inner.dst_client_id,
                src_packet_sequences: inner.src_packet_sequences,
                dst_packet_sequences: inner.dst_packet_sequences,
            }))
            .await?
            .into_inner();
        Ok(Response::new(api::RelayByTxResponse {
            tx: resp.tx,
            address: resp.address,
        }))
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        let checksum = hex::decode(request.get_ref().parameters.get(CHECKSUM_HEX).ok_or_else(
            || tonic::Status::internal("Checksum hex parameter is missing in request"),
        )?)
        .map_err(|e| tonic::Status::internal(format!("Failed to decode checksum hex: {e}")))?;
        Self::ensure_v1_3_checksum(&checksum)?;

        tracing::info!("Using Ethereum to Cosmos v1.3.0 create_client");
        let inner = request.into_inner();
        let resp = self
            .v1_3_service
            .create_client(Request::new(api_v1_3::CreateClientRequest {
                src_chain: inner.src_chain,
                dst_chain: inner.dst_chain,
                parameters: inner.parameters,
            }))
            .await?
            .into_inner();
        Ok(Response::new(api::CreateClientResponse {
            tx: resp.tx,
            address: resp.address,
        }))
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        let req = request.get_ref();
        let checksum = self.client_checksum(&req.dst_client_id).await?;
        Self::ensure_v1_3_checksum(&checksum)?;

        tracing::info!("Using Ethereum to Cosmos v1.3.0 update_client");
        let inner = request.into_inner();
        let resp = self
            .v1_3_service
            .update_client(Request::new(api_v1_3::UpdateClientRequest {
                src_chain: inner.src_chain,
                dst_chain: inner.dst_chain,
                dst_client_id: inner.dst_client_id,
            }))
            .await?
            .into_inner();
        Ok(Response::new(api::UpdateClientResponse {
            tx: resp.tx,
            address: resp.address,
        }))
    }
}

#[tonic::async_trait]
impl ProofApiModule for EthToCosmosV1_3_0ProofApiModule {
    fn name(&self) -> &'static str {
        "eth_to_cosmos_v1.3.0"
    }

    #[tracing::instrument(skip_all, err(Debug))]
    async fn create_service(&self, config: Value) -> anyhow::Result<Box<dyn ProofApiService>> {
        let tm_rpc_url = config
            .get("tm_rpc_url")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("eth_to_cosmos_v1.3.0 config must include tm_rpc_url"))?
            .to_string();
        let legacy_config = legacy_config(&config)?;

        let v1_3_service = EthToCosmosRelayerModuleV1_3
            .create_service(legacy_config)
            .await?;

        tracing::info!("Starting Ethereum to Cosmos v1.3.0 proof API server.");
        Ok(Box::new(EthToCosmosV1_3_0ProofApiModuleService::new(
            &tm_rpc_url,
            v1_3_service,
        )))
    }
}

fn legacy_config(eth_to_cosmos_config: &Value) -> anyhow::Result<Value> {
    let mode = eth_to_cosmos_config
        .get("mode")
        .ok_or_else(|| anyhow::anyhow!("eth_to_cosmos_v1.3.0 config must include mode"))?;
    let mock = match mode {
        Value::String(mode) if mode == "real" => false,
        Value::String(mode) if mode == "mock" => true,
        Value::Object(mode) if mode.get("type").and_then(Value::as_str) == Some("attested") => {
            anyhow::bail!("eth_to_cosmos_v1.3.0 does not support attested mode")
        }
        _ => anyhow::bail!("eth_to_cosmos_v1.3.0 mode must be `real` or `mock`"),
    };

    let mut config = eth_to_cosmos_config.clone();
    config["mock"] = Value::Bool(mock);
    config
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("eth_to_cosmos_v1.3.0 config must be an object"))?
        .remove("mode");
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::{V1_3_CHECKSUM, V1_3_CHECKSUM_HEX};

    #[test]
    fn v1_3_checksum_matches_expected_hex() {
        assert_eq!(
            hex::decode(V1_3_CHECKSUM_HEX).expect("valid checksum hex"),
            V1_3_CHECKSUM
        );
    }
}
