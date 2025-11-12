use anchor_lang::prelude::*;

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug, Default,
)]
pub enum AccessManagerVersion {
    #[default]
    V1,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace, Debug)]
pub struct RoleData {
    pub role_id: u64,
    #[max_len(32)]
    pub members: Vec<Pubkey>,
}

#[error_code]
pub enum AccessManagerError {
    #[msg("Unauthorized: caller does not have required role")]
    Unauthorized,
    #[msg("Not admin: only admin can perform this action")]
    NotAdmin,
    #[msg("Invalid role ID")]
    InvalidRoleId,
}
