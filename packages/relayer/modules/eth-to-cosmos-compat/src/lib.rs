//! Cosmos to Ethereum backwards compatible relayer module. This should be removed in the future

#![deny(
    clippy::nursery,
    clippy::pedantic,
    warnings,
    missing_docs,
    unused_crate_dependencies
)]

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};
use ibc_eureka_relayer_core_v1_2::{
    api::{self as api_v1_2, relayer_service_server::RelayerService as RelayerServiceV1_2},
    modules::RelayerModule as RelayerModuleV1_2,
};
use ibc_eureka_relayer_eth_to_cosmos::{EthToCosmosConfig, EthToCosmosRelayerModule};
use ibc_eureka_relayer_eth_to_cosmos_v1_2::EthToCosmosRelayerModule as EthToCosmosRelayerModuleV1_2;
use ibc_eureka_relayer_lib::listener::cosmos_sdk;
use ibc_eureka_utils::rpc::TendermintRpcExt;
use ibc_proto_eureka::ibc::lightclients::wasm::v1::ClientState;
use prost::Message;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

/// The checksum for the v1.2 Ethereum wasm client
const V1_2_CHECKSUM: &[u8] = &[
    185, 46, 153, 4, 170, 178, 41, 41, 22, 80, 127, 13, 176, 75, 122, 182, 208, 36, 194, 253, 181,
    122, 157, 82, 230, 114, 95, 105, 178, 230, 132, 193,
];

/// The key for the checksum hex in the parameters map.
const CHECKSUM_HEX: &str = "checksum_hex";

/// The `EthToCosmosCompatRelayerModule` struct defines the Ethereum to Cosmos backwards compatibility relayer module.
#[derive(Clone, Copy, Debug)]
pub struct EthToCosmosCompatRelayerModule;

/// The `EthereumToCosmosCompatRelayerModuleService` defines backwards compatibility the relayer service from Ethereum to Cosmos.
struct EthToCosmosCompatRelayerModuleService {
    /// The chain listener for Cosmos SDK.
    pub tm_listener: cosmos_sdk::ChainListener,
    pub old_service: Box<dyn RelayerServiceV1_2>,
    pub new_service: Box<dyn RelayerService>,
}

impl EthToCosmosCompatRelayerModuleService {
    fn new(
        config: &EthToCosmosConfig,
        old_service: Box<dyn RelayerServiceV1_2>,
        new_service: Box<dyn RelayerService>,
    ) -> Self {
        let tm_client = HttpClient::from_rpc_url(&config.tm_rpc_url);
        let tm_listener = cosmos_sdk::ChainListener::new(tm_client);
        Self {
            tm_listener,
            old_service,
            new_service,
        }
    }
}

#[tonic::async_trait]
impl RelayerService for EthToCosmosCompatRelayerModuleService {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        self.new_service.info(request).await
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        let req = request.get_ref();
        let client_state = self
            .tm_listener
            .client()
            .client_state(req.dst_client_id.clone())
            .await
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to get client state of {}: {e}",
                    req.dst_client_id
                ))
            })?;

        let checksum = ClientState::decode(&*client_state.value)
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to decode client state of {}: {e}",
                    req.dst_client_id
                ))
            })?
            .checksum;

        if checksum == V1_2_CHECKSUM {
            tracing::info!("Using bacwards compatible relay_by_tx",);
            let inner = request.into_inner();
            let resp = self
                .old_service
                .relay_by_tx(Request::new(api_v1_2::RelayByTxRequest {
                    src_chain: inner.src_chain,
                    dst_chain: inner.dst_chain,
                    dst_client_id: inner.dst_client_id,
                    src_client_id: inner.src_client_id,
                    source_tx_ids: inner.source_tx_ids,
                    timeout_tx_ids: inner.timeout_tx_ids,
                    src_packet_sequences: inner.src_packet_sequences,
                    dst_packet_sequences: inner.dst_packet_sequences,
                }))
                .await?
                .into_inner();
            Ok(Response::new(api::RelayByTxResponse {
                tx: resp.tx,
                address: resp.address,
            }))
        } else {
            self.new_service.relay_by_tx(request).await
        }
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        let checksum = hex::decode(request.get_ref().parameters.get(CHECKSUM_HEX).ok_or_else(
            || tonic::Status::internal("Checksum hex parameter is missing in request"),
        )?)
        .map_err(|e| tonic::Status::internal(format!("Failed to decode checksum hex: {e}")))?;

        if checksum == V1_2_CHECKSUM {
            tracing::info!("Using bacwards compatible create_client",);
            let inner = request.into_inner();
            let resp = self
                .old_service
                .create_client(Request::new(api_v1_2::CreateClientRequest {
                    dst_chain: inner.dst_chain,
                    src_chain: inner.src_chain,
                    parameters: inner.parameters,
                }))
                .await?
                .into_inner();
            Ok(Response::new(api::CreateClientResponse {
                tx: resp.tx,
                address: resp.address,
            }))
        } else {
            self.new_service.create_client(request).await
        }
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        let req = request.get_ref();
        let client_state = self
            .tm_listener
            .client()
            .client_state(req.dst_client_id.clone())
            .await
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to get client state of {}: {e}",
                    req.dst_client_id
                ))
            })?;

        let checksum = ClientState::decode(&*client_state.value)
            .map_err(|e| {
                tonic::Status::internal(format!(
                    "Failed to decode client state of {}: {e}",
                    req.dst_client_id
                ))
            })?
            .checksum;

        if checksum == V1_2_CHECKSUM {
            tracing::info!("Using bacwards compatible update_client",);
            let inner = request.into_inner();
            let resp = self
                .old_service
                .update_client(Request::new(api_v1_2::UpdateClientRequest {
                    dst_client_id: inner.dst_client_id,
                    dst_chain: inner.dst_chain,
                    src_chain: inner.src_chain,
                }))
                .await?
                .into_inner();
            Ok(Response::new(api::UpdateClientResponse {
                tx: resp.tx,
                address: resp.address,
            }))
        } else {
            self.new_service.update_client(request).await
        }
    }
}

#[tonic::async_trait]
impl RelayerModule for EthToCosmosCompatRelayerModule {
    fn name(&self) -> &'static str {
        "eth_to_cosmos_compat"
    }

    #[tracing::instrument(skip_all)]
    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let old_service = EthToCosmosRelayerModuleV1_2
            .create_service(config.clone())
            .await?;

        let new_service = EthToCosmosRelayerModule
            .create_service(config.clone())
            .await?;

        let config = serde_json::from_value::<EthToCosmosConfig>(config)
            .map_err(|e| anyhow::anyhow!("failed to parse config: {e}"))?;

        tracing::info!("Starting Ethereum to Cosmos bacwards compatible relayer server.");
        Ok(Box::new(EthToCosmosCompatRelayerModuleService::new(
            &config,
            old_service,
            new_service,
        )))
    }
}
