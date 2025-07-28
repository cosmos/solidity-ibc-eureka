//! The crate that contains the types and utilities for `tendermint-light-client-update-client`
//! program.
#![deny(
    missing_docs,
    clippy::nursery,
    clippy::pedantic,
    warnings,
    unused_crate_dependencies
)]

pub mod types;

use std::{str::FromStr, time::Duration};

use ibc_client_tendermint::{
    client_state::verify_header,
    types::{ConsensusState, Header, TENDERMINT_CLIENT_TYPE},
};
use ibc_core_client_types::Height;
use ibc_core_host_types::identifiers::{ChainId, ClientId};
use tendermint_light_client_verifier::{
    options::Options, types::TrustThreshold as TmTrustThreshold, ProdVerifier,
};

/// Trust threshold
#[derive(Clone, Debug)]
pub struct TrustThreshold {
    /// Numerator of the fraction
    pub numerator: u64,
    /// Denominator of the fraction
    pub denominator: u64,
}

impl TrustThreshold {
    /// Create a new trust threshold
    #[must_use]
    pub const fn new(numerator: u64, denominator: u64) -> Self {
        Self {
            numerator,
            denominator,
        }
    }
}

impl From<TrustThreshold> for TmTrustThreshold {
    fn from(tt: TrustThreshold) -> Self {
        Self::new(tt.numerator, tt.denominator)
            .expect("trust threshold numerator must be less than or equal to denominator")
    }
}

/// Client state
#[derive(Clone, Debug)]
pub struct ClientState {
    /// Chain ID
    pub chain_id: String,
    /// Trust level
    pub trust_level: TrustThreshold,
    /// Trusting period in seconds
    pub trusting_period_seconds: u64,
    /// Unbonding period in seconds
    pub unbonding_period_seconds: u64,
    /// Max clock drift in seconds
    pub max_clock_drift_seconds: u64,
    /// Frozen height (None if not frozen)
    pub is_frozen: bool,
    /// Latest height
    pub latest_height: Height,
}

/// Output from update client verification
#[derive(Clone, Debug)]
pub struct UpdateClientOutput {
    /// New consensus state from the verified header
    pub new_consensus_state: ConsensusState,
    /// New latest height
    pub latest_height: Height,
    /// The trusted height used for verification
    pub trusted_height: Height,
}

/// Error type for update client
#[derive(Debug, thiserror::Error)]
pub enum UpdateClientError {
    /// Invalid client ID
    #[error("invalid client ID")]
    InvalidClientId,
    /// Invalid chain ID
    #[error("invalid chain ID: {0}")]
    InvalidChainId(String),
    /// Header verification failed
    #[error("header verification failed")]
    HeaderVerificationFailed,
}

/// IBC light client update client
///
/// # Errors
///
/// This function will return an error if:
/// - The client ID cannot be created
/// - The chain ID is invalid
/// - Header verification fails
pub fn update_client(
    client_state: &ClientState,
    trusted_consensus_state: &ConsensusState,
    proposed_header: Header,
    time: u128,
) -> Result<UpdateClientOutput, UpdateClientError> {
    let client_id =
        ClientId::new(TENDERMINT_CLIENT_TYPE, 0).map_err(|_| UpdateClientError::InvalidClientId)?;
    let chain_id = ChainId::from_str(&client_state.chain_id)
        .map_err(|_| UpdateClientError::InvalidChainId(client_state.chain_id.clone()))?;

    let trust_threshold: TmTrustThreshold = client_state.trust_level.clone().into();

    let options = Options {
        trust_threshold,
        trusting_period: Duration::from_secs(client_state.trusting_period_seconds),
        clock_drift: Duration::from_secs(client_state.max_clock_drift_seconds),
    };

    let mut ctx = types::validation::ClientValidationCtx::new(time);

    ctx.insert_trusted_consensus_state(
        client_id.clone(),
        proposed_header.trusted_height.revision_number(),
        proposed_header.trusted_height.revision_height(),
        trusted_consensus_state,
    );

    verify_header::<_, sha2::Sha256>(
        &ctx,
        &proposed_header,
        &client_id,
        &chain_id,
        &options,
        &ProdVerifier::default(),
    )
    .map_err(|_| UpdateClientError::HeaderVerificationFailed)?;

    let trusted_height = proposed_header.trusted_height;
    let latest_height = proposed_header.height();
    let new_consensus_state = ConsensusState::from(proposed_header);

    Ok(UpdateClientOutput {
        new_consensus_state,
        latest_height,
        trusted_height,
    })
}
