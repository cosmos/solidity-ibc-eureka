//! Solidity types for ICS26Router.sol

#[cfg(feature = "rpc")]
alloy_sol_types::sol!(
    #[sol(rpc)]
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    #[allow(missing_docs, clippy::pedantic, warnings)]
    router,
    "../../abi/ICS26Router.json"
);

// NOTE: Some environments won't compile with the `rpc` features.
#[cfg(not(feature = "rpc"))]
alloy_sol_types::sol!(
    #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
    #[allow(missing_docs, clippy::pedantic)]
    router,
    "../../abi/ICS26Router.json"
);
