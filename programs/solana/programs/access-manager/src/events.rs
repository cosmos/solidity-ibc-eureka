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
pub struct AdminUpdatedEvent {
    pub old_admin: Pubkey,
    pub new_admin: Pubkey,
    pub updated_by: Pubkey,
}
