use anchor_lang::prelude::*;

#[event]
#[derive(Debug, Clone)]
pub struct RoleGrantedEvent {
    pub role_id: u64,
    pub account: Pubkey,
    pub granted_by: Pubkey,
}

#[event]
#[derive(Debug, Clone)]
pub struct RoleRevokedEvent {
    pub role_id: u64,
    pub account: Pubkey,
    pub revoked_by: Pubkey,
}

#[event]
#[derive(Debug, Clone)]
pub struct ProgramUpgradedEvent {
    pub program: Pubkey,
    pub authority: Pubkey,
    pub timestamp: i64,
}

#[event]
#[derive(Debug, Clone)]
pub struct ProgramExtendedEvent {
    pub program: Pubkey,
    pub authority: Pubkey,
    pub additional_bytes: u32,
    pub timestamp: i64,
}
