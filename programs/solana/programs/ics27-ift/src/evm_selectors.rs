//! EVM function selectors generated at compile time.
//!
//! These are the first 4 bytes of `keccak256(function_signature)`.

include!(concat!(env!("OUT_DIR"), "/evm_selectors.rs"));
