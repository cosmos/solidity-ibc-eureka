//! This is a one-sided relayer module from a Cosmos SDK chain to Solana.

#![deny(clippy::nursery, clippy::pedantic, warnings, unused_crate_dependencies)]
#![allow(missing_docs, unused_crate_dependencies)]

pub mod tx_builder;

use std::collections::HashMap;
use std::sync::Arc;

use ibc_eureka_utils::rpc::TendermintRpcExt;
use solana_client::rpc_client::RpcClient;
use tendermint::Hash;
use tendermint_rpc::Client;
use tendermint_rpc::HttpClient;
use tonic::{Request, Response};

use ibc_eureka_relayer_core::{
    api::{self, relayer_service_server::RelayerService},
    modules::RelayerModule,
};

use tx_builder::CosmosIbcEvent;

/// The `CosmosToSolanaRelayerModule` struct defines the Cosmos to Solana relayer module.
#[derive(Clone, Copy, Debug)]
pub struct CosmosToSolanaRelayerModule;

/// Transaction builder for Cosmos to Solana relaying
enum CosmosToSolanaTxBuilder {
    Real(tx_builder::TxBuilder),
    Mock(tx_builder::MockTxBuilder),
}

/// The `CosmosToSolanaRelayerModuleService` defines the relayer service from Cosmos to Solana.
#[allow(dead_code)]
struct CosmosToSolanaRelayerModuleService {
    /// The source Cosmos tendermint client.
    pub source_tm_client: HttpClient,
    /// The target Solana RPC client
    pub solana_client: Arc<RpcClient>,
    /// The transaction builder from Cosmos to Solana.
    pub tx_builder: CosmosToSolanaTxBuilder,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: solana_sdk::pubkey::Pubkey,
    /// The Solana ICS07 Tendermint light client program ID.
    pub solana_ics07_program_id: solana_sdk::pubkey::Pubkey,
}

/// The configuration for the Cosmos to Solana relayer module.
#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct CosmosToSolanaConfig {
    /// The source tendermint RPC URL.
    pub source_rpc_url: String,
    /// The Solana RPC URL.
    pub solana_rpc_url: String,
    /// The Solana ICS26 router program ID.
    pub solana_ics26_program_id: String,
    /// The Solana ICS07 Tendermint light client program ID.
    pub solana_ics07_program_id: String,
    /// The Solana fee payer address.
    pub solana_fee_payer: String,
    /// Whether to use mock WASM client on Cosmos for testing.
    pub mock_wasm_client: bool,
    /// Whether to use mock Solana light client updates for testing.
    pub mock_solana_client: bool,
}

impl CosmosToSolanaRelayerModuleService {
    fn new(config: &CosmosToSolanaConfig) -> anyhow::Result<Self> {
        let source_client = HttpClient::from_rpc_url(&config.source_rpc_url);
        let solana_client = Arc::new(RpcClient::new(config.solana_rpc_url.clone()));

        let solana_ics26_program_id = config
            .solana_ics26_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS26 program ID: {}", e))?;

        let solana_ics07_program_id = config
            .solana_ics07_program_id
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid Solana ICS07 program ID: {}", e))?;

        let fee_payer = config
            .solana_fee_payer
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid fee payer address: {}", e))?;

        let tx_builder = if config.mock_solana_client {
            CosmosToSolanaTxBuilder::Mock(tx_builder::MockTxBuilder::new(
                source_client.clone(),
                solana_client.clone(),
                solana_ics26_program_id,
                solana_ics07_program_id,
                fee_payer,
            )?)
        } else {
            CosmosToSolanaTxBuilder::Real(tx_builder::TxBuilder::new(
                source_client.clone(),
                solana_client.clone(),
                solana_ics26_program_id,
                solana_ics07_program_id,
                fee_payer,
            )?)
        };

        Ok(Self {
            source_tm_client: source_client,
            solana_client,
            tx_builder,
            solana_ics26_program_id,
            solana_ics07_program_id,
        })
    }
}

impl CosmosToSolanaTxBuilder {
    /// Fetch events from Cosmos transactions
    async fn fetch_cosmos_events(
        &self,
        cosmos_txs: Vec<tendermint::Hash>,
    ) -> anyhow::Result<Vec<tx_builder::CosmosIbcEvent>> {
        match self {
            Self::Real(tb) => tb.fetch_cosmos_events(cosmos_txs).await,
            Self::Mock(tb) => tb.fetch_cosmos_events(cosmos_txs).await,
        }
    }

    /// Fetch timeout events from Solana
    fn fetch_solana_timeout_events(
        &self,
        solana_txs: Vec<solana_sdk::signature::Signature>,
    ) -> anyhow::Result<Vec<tx_builder::CosmosIbcEvent>> {
        match self {
            Self::Real(tb) => tb.fetch_solana_timeout_events(solana_txs),
            Self::Mock(tb) => tb.fetch_solana_timeout_events(solana_txs),
        }
    }

    /// Build Solana transaction for relaying
    async fn build_solana_tx(
        &self,
        src_events: Vec<tx_builder::CosmosIbcEvent>,
        target_events: Vec<tx_builder::CosmosIbcEvent>,
    ) -> anyhow::Result<solana_sdk::transaction::Transaction> {
        match self {
            Self::Real(tb) => tb.build_solana_tx(src_events, target_events).await,
            Self::Mock(tb) => tb.build_solana_tx(src_events, target_events).await,
        }
    }

    /// Build create client transaction
    async fn build_create_client_tx(&self) -> anyhow::Result<solana_sdk::transaction::Transaction> {
        match self {
            Self::Real(tb) => tb.build_create_client_tx().await,
            Self::Mock(tb) => tb.build_create_client_tx().await,
        }
    }

    /// Build update client transaction
    async fn build_update_client_tx(
        &self,
        client_id: String,
    ) -> anyhow::Result<solana_sdk::transaction::Transaction> {
        match self {
            Self::Real(tb) => tb.build_update_client_tx(client_id).await,
            Self::Mock(tb) => tb.build_update_client_tx(client_id).await,
        }
    }
}

#[tonic::async_trait]
impl RelayerService for CosmosToSolanaRelayerModuleService {
    #[tracing::instrument(skip_all)]
    async fn info(
        &self,
        _request: Request<api::InfoRequest>,
    ) -> Result<Response<api::InfoResponse>, tonic::Status> {
        tracing::info!("Handling info request for Cosmos to Solana...");

        // Get Cosmos chain ID
        let status = self
            .source_tm_client
            .status()
            .await
            .map_err(|e| tonic::Status::internal(format!("Failed to get chain ID: {e}")))?;

        Ok(Response::new(api::InfoResponse {
            source_chain: Some(api::Chain {
                chain_id: status.node_info.network.to_string(),
                ibc_version: "2".to_string(),
                ibc_contract: String::new(),
            }),
            target_chain: Some(api::Chain {
                chain_id: "solana".to_string(), // Solana doesn't have chain IDs like Cosmos
                ibc_version: "2".to_string(),
                ibc_contract: self.solana_ics26_program_id.to_string(),
            }),
            metadata: HashMap::default(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn relay_by_tx(
        &self,
        request: Request<api::RelayByTxRequest>,
    ) -> Result<Response<api::RelayByTxResponse>, tonic::Status> {
        tracing::info!("Handling relay by tx request for Cosmos to Solana...");

        let inner_req = request.into_inner();
        tracing::info!("Got {} source tx IDs", inner_req.source_tx_ids.len());
        tracing::info!("Got {} timeout tx IDs", inner_req.timeout_tx_ids.len());

        // Parse Cosmos transaction hashes
        let src_txs = inner_req
            .source_tx_ids
            .into_iter()
            .map(Hash::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        // Parse Solana transaction signatures for timeouts
        let target_txs: Vec<solana_sdk::signature::Signature> = inner_req
            .timeout_tx_ids
            .into_iter()
            .map(|tx_id| {
                let sig_str = String::from_utf8(tx_id).map_err(|e| {
                    tonic::Status::invalid_argument(format!("Invalid signature: {e}"))
                })?;
                sig_str
                    .parse::<solana_sdk::signature::Signature>()
                    .map_err(|e| tonic::Status::invalid_argument(format!("Invalid signature: {e}")))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Fetch events from Cosmos transactions
        let src_events = self
            .tx_builder
            .fetch_cosmos_events(src_txs)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(cosmos_src_events = ?src_events, "Fetched source Cosmos events.");
        tracing::info!(
            "Fetched {} source eureka events from Cosmos.",
            src_events.len()
        );
        
        // Log details about each source event
        for (i, event) in src_events.iter().enumerate() {
            match event {
                CosmosIbcEvent::SendPacket { sequence, source_client, destination_client, payloads, timeout_timestamp } => {
                    tracing::info!("Event[{}]: SendPacket seq={}, src_client={}, dest_client={}, timeout={}, payloads_count={}",
                        i, sequence, source_client, destination_client, timeout_timestamp, payloads.len());
                    for (j, payload) in payloads.iter().enumerate() {
                        tracing::info!("  Payload[{}]: port={}=>{}, version={}, encoding={}, value_len={}",
                            j, payload.source_port, payload.dest_port, payload.version, payload.encoding, payload.value.len());
                        
                        // Highlight suspicious encoding
                        if payload.encoding == "application/x-solidity-abi" {
                            tracing::warn!("    ⚠️  SUSPICIOUS: Solidity ABI encoding in Cosmos->Solana transfer!");
                        }
                        
                        // Warn about large payloads
                        if payload.value.len() > 200 {
                            tracing::warn!("    ⚠️  LARGE PAYLOAD: {} bytes - this contributes to transaction size!", payload.value.len());
                        }
                        
                        // Show first 200 chars of payload value for analysis  
                        let value_preview = if payload.value.len() > 200 {
                            format!("{}... (truncated)", String::from_utf8_lossy(&payload.value[..200]))
                        } else {
                            String::from_utf8_lossy(&payload.value).to_string()
                        };
                        tracing::info!("    Value: {}", value_preview);
                    }
                }
                _ => {
                    tracing::info!("Event[{}]: {:?}", i, event);
                }
            }
        }

        // Fetch events from Solana for timeouts
        let target_events = self
            .tx_builder
            .fetch_solana_timeout_events(target_txs)
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::debug!(solana_target_events = ?target_events, "Fetched target Solana events.");
        tracing::info!(
            "Fetched {} target eureka events from Solana.",
            target_events.len()
        );

        let tx = self
            .tx_builder
            .build_solana_tx(src_events, target_events)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        tracing::info!("Built Solana transaction for Cosmos to Solana relay.");

        // Debug: log transaction details before serialization
        tracing::info!(
            "Transaction instructions count: {}",
            tx.message.instructions.len()
        );
        tracing::info!(
            "Transaction accounts count: {}",
            tx.message.account_keys.len()
        );
        tracing::info!("Transaction signatures count: {}", tx.signatures.len());

        // Log all account keys first
        for (i, account_key) in tx.message.account_keys.iter().enumerate() {
            tracing::info!("Account[{}]: {}", i, account_key);
        }
        
        for (i, instruction) in tx.message.instructions.iter().enumerate() {
            let program_account = tx.message.account_keys.get(instruction.program_id_index as usize)
                .map(|pk| pk.to_string())
                .unwrap_or_else(|| "UNKNOWN".to_string());
            
            // Identify known programs
            let program_name = if program_account == "CSLS3A9jS7JAD8aUe3LRXMYZ1U8Lvxn9usGygVrA2arZ" {
                "ICS07-Tendermint-Light-Client"
            } else if program_account == "FRGF7cthWUvDvAHMUARUHFycyUK2VDUtBchmkwrz7hgx" {
                "ICS26-Router"
            } else if program_account == "11111111111111111111111111111111" {
                "System-Program"
            } else if program_account == "5E73beFMq9QZvbwPN5i84psh2WcyJ9PgqF4avBaRDgCC" {
                "Dummy-IBC-App"
            } else {
                "Unknown-Program"
            };
            
            // Translate account indices to actual pubkeys
            let account_keys: Vec<String> = instruction.accounts.iter()
                .map(|&idx| {
                    tx.message.account_keys.get(idx as usize)
                        .map(|pk| pk.to_string())
                        .unwrap_or_else(|| format!("INVALID[{}]", idx))
                })
                .collect();
            
            tracing::info!(
                "Instruction {}: program={} [{}] (index={}), accounts_count={}, data_len={}",
                i,
                program_name,
                program_account,
                instruction.program_id_index,
                instruction.accounts.len(),
                instruction.data.len()
            );
            
            // Show account keys
            for (j, account_key) in account_keys.iter().enumerate() {
                tracing::info!("  Account[{}]: {}", j, account_key);
            }
            
            // Show first 100 bytes of instruction data in hex for analysis
            if instruction.data.len() > 0 {
                let data_preview = if instruction.data.len() > 100 {
                    format!("{}... (truncated)", hex::encode(&instruction.data[..100]))
                } else {
                    hex::encode(&instruction.data)
                };
                tracing::info!("  Data (hex): {}", data_preview);
                
                // Analyze large instruction data (likely recv packet)
                if instruction.data.len() > 100 {
                    tracing::info!("  Large instruction data analysis ({} bytes):", instruction.data.len());
                    
                    // Check if it's a Solana IBC packet by looking at the structure
                    // The data might be: [instruction_discriminator][serialized_packet_data]
                    if instruction.data.len() > 8 {
                        let discriminator = &instruction.data[..8];
                        let payload_data = &instruction.data[8..];
                        tracing::info!("  Discriminator: {}", hex::encode(discriminator));
                        tracing::info!("  Payload data size: {} bytes", payload_data.len());
                        
                        // Look for readable strings in the data to understand structure
                        let data_str = String::from_utf8_lossy(&instruction.data);
                        let readable_parts: Vec<&str> = data_str
                            .split('\0')
                            .filter(|s| s.len() > 2 && s.chars().all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace()))
                            .collect();
                        
                        if !readable_parts.is_empty() {
                            tracing::info!("  Readable strings found in data:");
                            for (i, part) in readable_parts.iter().enumerate() {
                                tracing::info!("    [{}]: '{}'", i, part);
                            }
                        }
                        
                        // Show more detailed hex breakdown of first 200 bytes
                        let detailed_hex = if instruction.data.len() > 200 {
                            hex::encode(&instruction.data[..200])
                        } else {
                            hex::encode(&instruction.data)
                        };
                        tracing::info!("  Detailed hex (first {} bytes): {}", 
                            std::cmp::min(instruction.data.len(), 200), detailed_hex);
                    }
                }
            }
        }

        // Serialize the unsigned transaction
        let tx_bytes = bincode::serialize(&tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize transaction: {e}"))
        })?;

        tracing::info!("Serialized transaction size: {} bytes", tx_bytes.len());

        Ok(Response::new(api::RelayByTxResponse {
            tx: tx_bytes,
            address: self.solana_ics26_program_id.to_string(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn create_client(
        &self,
        _request: Request<api::CreateClientRequest>,
    ) -> Result<Response<api::CreateClientResponse>, tonic::Status> {
        tracing::info!("Handling create client request for Cosmos to Solana...");

        let tx = self
            .tx_builder
            .build_create_client_tx()
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        let tx_bytes = bincode::serialize(&tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize transaction: {e}"))
        })?;

        Ok(Response::new(api::CreateClientResponse {
            tx: tx_bytes,
            address: self.solana_ics07_program_id.to_string(),
        }))
    }

    #[tracing::instrument(skip_all)]
    async fn update_client(
        &self,
        request: Request<api::UpdateClientRequest>,
    ) -> Result<Response<api::UpdateClientResponse>, tonic::Status> {
        tracing::info!("Handling update client request for Cosmos to Solana...");

        let inner_req = request.into_inner();

        let tx = self
            .tx_builder
            .build_update_client_tx(inner_req.dst_client_id)
            .await
            .map_err(|e| tonic::Status::from_error(e.into()))?;

        let tx_bytes = bincode::serialize(&tx).map_err(|e| {
            tonic::Status::internal(format!("Failed to serialize transaction: {e}"))
        })?;

        Ok(Response::new(api::UpdateClientResponse {
            tx: tx_bytes,
            address: self.solana_ics07_program_id.to_string(),
        }))
    }
}

#[tonic::async_trait]
impl RelayerModule for CosmosToSolanaRelayerModule {
    fn name(&self) -> &'static str {
        "cosmos_to_solana"
    }

    async fn create_service(
        &self,
        config: serde_json::Value,
    ) -> anyhow::Result<Box<dyn RelayerService>> {
        let config: CosmosToSolanaConfig = serde_json::from_value(config)?;
        let service = CosmosToSolanaRelayerModuleService::new(&config)?;
        Ok(Box::new(service))
    }
}
