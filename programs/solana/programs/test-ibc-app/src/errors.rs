use anchor_lang::prelude::*;

#[error_code]
pub enum TestIbcAppError {
    #[msg("Unauthorized: Only the IBC router can call this instruction")]
    UnauthorizedCaller,
    #[msg("Invalid packet data")]
    InvalidPacketData,
}
