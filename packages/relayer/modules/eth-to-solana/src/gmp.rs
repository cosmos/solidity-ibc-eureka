//! GMP constants, ABI types and shared protobuf re-exports for Eth→Solana relay.

pub use ibc_eureka_relayer_lib::utils::solana_gmp::{
    extract_gmp_accounts, find_gmp_result_pda, GMP_PORT_ID, PROTOBUF_ENCODING,
};

/// ABI encoding identifier used by Ethereum ICS27 GMP.
pub const ABI_ENCODING: &str = "application/x-solidity-abi";

// ABI type matching Solidity's `IICS27GMPMsgs.GMPPacketData`.
alloy::sol! {
    struct AbiGmpPacketData {
        string sender;
        string receiver;
        bytes salt;
        bytes payload;
        string memo;
    }
}
