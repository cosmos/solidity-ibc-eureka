//! Solana consensus layer types

use serde::{Deserialize, Serialize};

/// Fork parameters for Solana consensus
pub mod fork {
    use super::*;

    /// Minimal fork parameters for Solana consensus
    #[derive(
        Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
    )]
    pub struct ForkParameters;
}

/// Light client header types
pub mod light_client_header {
    use super::*;

    /// Minimal light client update structure
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct LightClientUpdate;
}
