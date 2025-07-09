//! Solana consensus layer types

use serde::{Deserialize, Serialize};

/// Fork parameters for Solana consensus
pub mod fork {
    use super::*;

    /// Minimal fork parameters for Solana consensus
    #[derive(
        Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default, schemars::JsonSchema,
    )]
    pub struct ForkParameters {
        /// Placeholder for future fork handling
        pub _placeholder: (),
    }
}

/// Light client header types
pub mod light_client_header {
    use super::*;

    /// Minimal light client update structure
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
    pub struct LightClientUpdate {
        /// Placeholder for future light client update data
        pub _placeholder: (),
    }
}
