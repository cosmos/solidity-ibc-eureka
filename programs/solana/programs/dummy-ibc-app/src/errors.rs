use anchor_lang::prelude::*;

#[error_code]
pub enum DummyIbcAppError {
    #[msg("Unauthorized caller - only ICS26 Router can call this instruction")]
    UnauthorizedCaller,
}
