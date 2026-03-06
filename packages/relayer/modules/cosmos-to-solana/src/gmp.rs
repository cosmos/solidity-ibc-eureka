//! GMP account extraction - delegates to shared `ibc_eureka_relayer_lib::utils::solana_gmp`.

pub use ibc_eureka_relayer_lib::utils::solana_gmp::{
    extract_gmp_accounts, extract_gmp_prefund_lamports, find_gmp_result_pda, GMP_PORT_ID,
    MAX_PREFUND_LAMPORTS, PROTOBUF_ENCODING,
};
