use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, InitSpace, Debug)]
pub struct RoleData {
    pub role_id: u64,
    #[max_len(16)]
    pub members: Vec<Pubkey>,
}
