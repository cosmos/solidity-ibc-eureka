//! Test helpers for Solana light client tests

use cosmwasm_std::testing::{mock_dependencies, MockApi, MockQuerier, MockStorage};
use cosmwasm_std::{Empty, OwnedDeps};

/// Mock dependencies for testing
#[must_use]
pub fn mk_deps() -> OwnedDeps<MockStorage, MockApi, MockQuerier, Empty> {
    mock_dependencies()
}
