//! This module contains the custom `CosmWasm` query for the Ethereum light client

use alloy_primitives::B256;
use cosmwasm_std::{Binary, CustomQuery, QuerierWrapper, QueryRequest};
use ethereum_light_client::types::bls::{BlsPublicKey, BlsSignature, BlsVerify};
use ethereum_utils::{ensure, hex::to_hex};
use thiserror::Error;

/// The custom query for the Ethereum light client
/// This is used to verify BLS signatures in `CosmosSDK`
#[derive(serde::Serialize, serde::Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::module_name_repetitions)]
pub enum EthereumCustomQuery {
    /// Verify a BLS signature
    AggregateVerify {
        /// The public keys to verify the signature
        public_keys: Vec<Binary>,
        /// The message to verify
        message: Binary,
        /// The signature to verify
        signature: Binary,
    },
    /// Aggregate public keys
    Aggregate {
        /// The public keys to aggregate
        public_keys: Vec<Binary>,
    },
}

impl CustomQuery for EthereumCustomQuery {}

/// The BLS verifier via [`EthereumCustomQuery`]
pub struct BlsVerifier<'a> {
    /// The `CosmWasm` querier
    pub querier: QuerierWrapper<'a, EthereumCustomQuery>,
}

/// The error type for the BLS verifier
#[derive(Error, Debug)]
#[allow(missing_docs)]
pub enum BlsVerifierError {
    #[error("fast aggregate verify error: {0}")]
    FastAggregateVerify(String),

    #[error("signature cannot be verified (public_keys: {public_keys:?}, msg: {msg}, signature: {signature})", msg = to_hex(.msg))]
    InvalidSignature {
        /// The public keys used to verify the signature
        public_keys: Vec<BlsPublicKey>,
        /// The message that was signed
        msg: B256,
        /// The signature that was verified
        signature: BlsSignature,
    },
}

impl BlsVerify for BlsVerifier<'_> {
    type Error = BlsVerifierError;

    fn fast_aggregate_verify(
        &self,
        public_keys: Vec<&BlsPublicKey>,
        msg: B256,
        signature: BlsSignature,
    ) -> Result<(), Self::Error> {
        let binary_public_keys: Vec<Binary> = public_keys
            .clone()
            .into_iter()
            .map(|p| Binary::from(p.to_vec()))
            .collect();

        let request: QueryRequest<EthereumCustomQuery> =
            QueryRequest::Custom(EthereumCustomQuery::AggregateVerify {
                public_keys: binary_public_keys,
                message: Binary::from(msg.to_vec()),
                signature: Binary::from(signature.to_vec()),
            });

        let is_valid: bool = self
            .querier
            .query(&request)
            .map_err(|e| BlsVerifierError::FastAggregateVerify(e.to_string()))?;

        ensure!(
            is_valid,
            BlsVerifierError::InvalidSignature {
                public_keys: public_keys.into_iter().copied().collect(),
                msg,
                signature,
            }
        );

        Ok(())
    }
}
