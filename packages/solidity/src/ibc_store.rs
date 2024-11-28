//! Solidity types for IBC Store

#[cfg(feature = "rpc")]
alloy_sol_types::sol!(
    #[sol(rpc)]
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    store,
    "../../abi/IBCStore.json"
);

// NOTE: Some environments won't compile with the `rpc` features.
#[cfg(not(feature = "rpc"))]
alloy_sol_types::sol!(
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    #[allow(missing_docs, clippy::pedantic)]
    store,
    "../../abi/IBCStore.json"
);
