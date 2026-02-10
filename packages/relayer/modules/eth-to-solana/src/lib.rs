//! One-sided relayer module from Ethereum to Solana.
//!
//! Listens for IBC events on EVM, translates IFT payloads (ABI → GmpSolanaPayload),
//! and builds Solana transactions with chunking, ALT, and attestation support.

#![deny(clippy::nursery, clippy::pedantic, warnings)]
#![allow(missing_docs, unused_crate_dependencies)]

pub mod constants;
pub mod gmp;
pub mod ift;
pub mod ift_payload;
pub mod proto;
pub mod tx_builder;

use std::collections::HashMap;

use alloy::{
    primitives::{Address, TxHash},
    providers::{Provider, RootProvider},
};
use ibc_eureka_relayer_lib::aggregator::Config as AggregatorConfig;
use ibc_eureka_relayer_lib::listener::{eth_eureka, solana, ChainListenerService};
use ibc_eureka_relayer_lib::service_utils::{
    parse_eth_tx_hashes, parse_solana_tx_hashes, to_tonic_status,
};
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

/// The `EthToSolanaRelayerModule` defines the Ethereum to Solana relayer module.
#[derive(Clone, Copy, Debug)]
pub struct EthToSolanaRelayerModule;

/// The relayer service from Ethereum to Solana.
struct EthToSolanaRelayerModuleService {
    /// Source chain listener for Ethereum.
    eth_listener: eth_eureka::ChainListener<RootProvider>,
    /// Target chain listener for Solana.
    target_listener: solana::ChainListener,
    /// Transaction builder.
    tx_builder: EthToSolanaTxBuilder,
}

/// Enum wrapping transaction builders for different modes.
enum EthToSolanaTxBuilder {
    /// Attestation light client mode.
    Attested(tx_builder::AttestedTxBuilder),
}

/// Configuration for the Ethereum to Solana relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct EthToSolanaConfig {
    /// The ICS26 contract address on Ethereum.
    pub ics26_address: Address,
    /// The EVM RPC URL.
    pub eth_rpc_url: String,
    /// The Solana RPC URL.
    pub solana_rpc_url: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
    /// The Solana fee payer address.
    pub solana_fee_payer: String,
    /// Address Lookup Table address for reducing transaction size (optional).
    pub solana_alt_address: Option<String>,
    /// Transaction builder mode.
    pub mode: EthToSolanaTxBuilderMode,
}

/// Transaction builder mode for Eth to Solana relay.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EthToSolanaTxBuilderMode {
    /// Attestation light client mode.
    Attested(AggregatorConfig),
}

impl EthToSolanaRelayerModuleService {
    async fn new(config: EthToSolanaConfig) -> anyhow::Result<Self> {
        let provider = RootProvider::builder()
            .connect(&config.eth_rpc_url)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create EVM provider: {e}"))?;

        let eth_listener = eth_eureka::ChainListener::new(config.ics26_address, provider);

        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS26 program ID: {e}"))?;

        let target_listener =
            solana::ChainListener::new(config.solana_rpc_url.clone(), solana_ics26_program_id);

        let fee_payer = config
            .solana_fee_payer
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid fee payer address: {e}"))?;

        let alt_address = config
            .solana_alt_address
            .as_ref()
            .map(|addr| addr.parse())
            .transpose()
            .map_err(|e| anyhow::anyhow!("Invalid ALT address: {e}"))?;

        let tx_builder = match config.mode {
            EthToSolanaTxBuilderMode::Attested(aggregator_config) => {
                let base_builder = tx_builder::SolanaTxBuilder::new(
                    target_listener.client().clone(),
                    solana_ics26_program_id,
                    fee_payer,
                    alt_address,
                )?;
                let attested_builder = tx_builder::AttestedTxBuilder::new(
                    aggregator_config,
                    base_builder,
                    config.ics26_address,
                )
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create attested tx builder: {e}"))?;
                EthToSolanaTxBuilder::Attested(attested_builder)
            }
        };

        Ok(Self {
            eth_listener,
            target_listener,
            tx_builder,
        })
    }
}

#[tonic::async_trait]
impl RelayerService for EthToSolanaRelayerModuleService {
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::debug!("Handling info request for Eth to Solana");

        Ok(Response::new(api::InfoResponse {
            source_chain: Some(api::Chain {
                chain_id: self
                    .eth_listener
                    .chain_id()
                    .await
                    .map_err(to_tonic_status)?,
                ibc_version: "2".to_string(),
                ibc_contract: self.tx_builder.ics26_eth_address().to_string(),
            }),
            target_chain: Some(api::Chain {
                chain_id: "solana-localnet".to_string(),
                ibc_version: "2".to_string(),
                ibc_contract: self.target_listener.ics26_program_id().to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        let inner_req = request.into_inner();
        tracing::debug!(
            "Relay request: {} source txs, {} timeout txs",
            inner_req.source_tx_ids.len(),
            inner_req.timeout_tx_ids.len()
        );

        let eth_tx_hashes = parse_eth_tx_hashes(inner_req.source_tx_ids)?;
        let eth_txs = eth_tx_hashes.into_iter().map(TxHash::from).collect();
        let target_txs = parse_solana_tx_hashes(inner_req.timeout_tx_ids)?;

        let src_events = self
            .eth_listener
            .fetch_tx_events(eth_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!("Fetched {} src events from EVM", src_events.len());

        let target_events = self
            .target_listener
            .fetch_tx_events(target_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!("Fetched {} target events from Solana", target_events.len());

        let (packet_txs, update_client) = self
            .tx_builder
            .relay_events(
                src_events,
                target_events,
                &inner_req.src_client_id,
                &inner_req.dst_client_id,
                &inner_req.src_packet_sequences,
                &inner_req.dst_packet_sequences,
            )
            .await
            .map_err(to_tonic_status)?;

        tracing::debug!(
            "Relay completed: {} packets{}",
            packet_txs.len(),
            if update_client.is_some() {
                " +update"
            } else {
                ""
            }
        );

        let batch = api::SolanaRelayPacketBatch {
            packets: packet_txs,
            update_client,
        };
        let tx = prost::Message::encode_to_vec(&batch);

        Ok(Response::new(api::RelayByTxResponse {
            tx,
            address: String::new(),
        }))
    }

    async fn create_client(
        &self,
        request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::debug!("Handling create client request for Eth to Solana");

        let inner_req = request.into_inner();
        let tx = self
            .tx_builder
            .create_client(&inner_req.parameters)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        Ok(Response::new(api::CreateClientResponse {
            tx,
            address: String::new(),
        }))
    }

    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Eth to Solana");

        let inner_req = request.into_inner();
        let solana_update_client = self
            .tx_builder
            .update_client(&inner_req.dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        let tx = prost::Message::encode_to_vec(&solana_update_client);

        Ok(Response::new(api::UpdateClientResponse {
            tx,
            address: String::new(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for EthToSolanaRelayerModule {
    fn name(&self) -> &'static str {
        "eth_to_solana"
    }

    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config: EthToSolanaConfig = serde_json::from_value(config)?;
        let service = EthToSolanaRelayerModuleService::new(config).await?;
        Ok(Box::new(service))
    }
}

impl EthToSolanaTxBuilder {
    async fn relay_events(
        &self,
        src_events: Vec<ibc_eureka_relayer_lib::events::EurekaEventWithHeight>,
        target_events: Vec<ibc_eureka_relayer_lib::events::SolanaEurekaEventWithHeight>,
        src_client_id: &str,
        dst_client_id: &str,
        src_packet_seqs: &[u64],
        dst_packet_seqs: &[u64],
    ) -> anyhow::Result<(
        Vec<ibc_eureka_relayer_core::api::SolanaPacketTxs>,
        Option<ibc_eureka_relayer_core::api::SolanaUpdateClient>,
    )> {
        match self {
            Self::Attested(tb) => {
                tb.relay_events(tx_builder::RelayParams {
                    src_events,
                    dest_events: target_events,
                    src_client_id: src_client_id.to_string(),
                    dst_client_id: dst_client_id.to_string(),
                    src_packet_seqs: src_packet_seqs.to_vec(),
                    dst_packet_seqs: dst_packet_seqs.to_vec(),
                })
                .await
            }
        }
    }

    async fn create_client(&self, parameters: &HashMap<String, String>) -> anyhow::Result<Vec<u8>> {
        match self {
            Self::Attested(tb) => tb.tx_builder().create_client(parameters).await,
        }
    }

    async fn update_client(
        &self,
        dst_client_id: &str,
    ) -> anyhow::Result<ibc_eureka_relayer_core::api::SolanaUpdateClient> {
        match self {
            Self::Attested(tb) => tb.update_client(dst_client_id).await,
        }
    }

    const fn ics26_eth_address(&self) -> &Address {
        match self {
            Self::Attested(tb) => tb.ics26_eth_address(),
        }
    }
}
