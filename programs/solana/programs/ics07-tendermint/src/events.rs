//! Event types for the ICS07 Tendermint light client program

use anchor_lang::prelude::*;

/// Event emitted when access manager is updated
#[event]
#[derive(Debug, Clone)]
pub struct AccessManagerUpdated {
    pub old_access_manager: Pubkey,
    pub new_access_manager: Pubkey,
}
