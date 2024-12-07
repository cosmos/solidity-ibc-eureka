use cosmwasm_std::StdError;
use ethereum_light_client::error::EthereumIBCError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Verify membership failed")]
    VerifyMembershipFailed(#[source] EthereumIBCError),

    #[error("Verify non-membership failed")]
    VerifyNonMembershipFailed(#[source] EthereumIBCError),

    #[error("Verify client message failed")]
    VerifyClientMessageFailed(#[source] EthereumIBCError),
}
