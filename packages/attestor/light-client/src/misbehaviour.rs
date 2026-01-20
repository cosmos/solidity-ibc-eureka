//! Misbehaviour detection (not implemented for minimal Solana client)

use crate::{
    client_state::ClientState, consensus_state::ConsensusState, error::SolanaIBCError,
    header::ActiveSyncCommittee,
};
use solana_types::consensus::light_client_header::LightClientUpdate;

/// Verify misbehaviour (not implemented)
/// # Errors
/// Always returns unimplemented error
pub fn verify_misbehaviour(
    _client_state: &ClientState,
    _consensus_state: &ConsensusState,
    _sync_committee: &ActiveSyncCommittee,
    _update_1: &LightClientUpdate,
    _update_2: &LightClientUpdate,
    _current_timestamp: u64,
) -> Result<(), SolanaIBCError> {
    todo!()
}
