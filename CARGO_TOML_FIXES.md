# Cargo.toml Feature Flag and Dependency Fixes

## Summary of Changes Made

### 1. `packages/solana/types/Cargo.toml`

**Issues Fixed:**
- ❌ Used hardcoded version instead of workspace version
- ❌ Missing `[features]` section
- ❌ Missing `serde` derive feature
- ❌ Missing `schemars` dependency for schema generation
- ❌ Included unnecessary dependencies (`solana-client`, `solana-account-decoder`)

**Changes Made:**
- ✅ Changed `version = "0.1.0"` → `version = { workspace = true }`
- ✅ Added `[features]` section with `default = []` and `test-utils = []`
- ✅ Added `features = ["derive"]` to `serde` dependency
- ✅ Added `schemars = { workspace = true, features = ["derive"] }`
- ✅ Removed unused `solana-client` and `solana-account-decoder` dependencies
- ✅ Added `[dev-dependencies]` section

### 2. `packages/solana/light-client/Cargo.toml`

**Issues Fixed:**
- ❌ Used hardcoded version instead of workspace version
- ❌ Empty `test-utils` feature (should include optional dependencies)
- ❌ Missing essential dependencies (`schemars`, `hex`, `sha2`)
- ❌ Missing optional dependencies for test-utils
- ❌ Used path-based instead of workspace-based dependency refs

**Changes Made:**
- ✅ Changed `version = "0.1.0"` → `version = { workspace = true }`
- ✅ Enhanced `test-utils` feature: `["dep:prost", "dep:ibc-proto-eureka"]`
- ✅ Added `features = ["derive"]` to `serde` dependency
- ✅ Added essential dependencies: `schemars`, `hex`, `sha2`
- ✅ Added optional dependencies for test-utils: `prost`, `ibc-proto-eureka`
- ✅ Changed `solana-types = { path = "../types" }` → `solana-types = { workspace = true }`

### 3. `programs/cw-ics08-wasm-sol/Cargo.toml`

**Issues Fixed:**
- ❌ Used hardcoded version instead of workspace version
- ❌ Used path-based instead of workspace-based dependency refs
- ❌ Used hardcoded Solana SDK version

**Changes Made:**
- ✅ Changed `version = "0.1.0"` → `version = { workspace = true }`
- ✅ Changed path-based refs to workspace refs:
  - `solana-light-client = { path = "..." }` → `solana-light-client = { workspace = true }`
  - `solana-types = { path = "..." }` → `solana-types = { workspace = true }`
- ✅ Changed `solana-sdk = "2.1"` → `solana-sdk = { workspace = true }`

### 4. Root `Cargo.toml` Workspace Dependencies

**Issues Fixed:**
- ❌ Missing `solana-account-decoder` in workspace dependencies
- ❌ Inconsistent Solana SDK versions (2.3.2, 2.2.1, 2.1, 2.3.1)

**Changes Made:**
- ✅ Added `solana-account-decoder = { version = "2.1", default-features = false }`
- ✅ Standardized all Solana SDK versions to `2.1`:
  - `solana-client = { version = "2.1", default-features = false }`
  - `solana-commitment-config = { version = "2.1", default-features = false }`
  - `solana-account-decoder = { version = "2.1", default-features = false }`
  - `solana-sdk = { version = "2.1", default-features = false }`

## Verification Checklist

### ✅ All packages now follow workspace patterns:
- Use `version = { workspace = true }` instead of hardcoded versions
- Use `{ workspace = true }` for all workspace-managed dependencies
- Consistent dependency versioning across the workspace

### ✅ Feature flags are properly defined:
- `solana-types`: `default = []`, `test-utils = []`
- `solana-light-client`: `default = []`, `test-utils = ["dep:prost", "dep:ibc-proto-eureka"]`
- Test utilities are properly gated behind optional dependencies

### ✅ Required features are specified:
- `serde = { workspace = true, features = ["derive"] }` for serialization
- `schemars = { workspace = true, features = ["derive"] }` for schema generation
- `cosmwasm-std = { workspace = true, features = ["std"] }` for CosmWasm
- `prost = { workspace = true, features = ["std"] }` for protobuf

### ✅ Dependencies are consistent:
- All Solana SDK crates use version `2.1`
- All workspace dependencies properly reference the workspace
- No hardcoded versions in package manifests

## Notes

1. **Feature Flag Pattern**: Following the same pattern as `ethereum-light-client` where `test-utils` feature enables optional dependencies needed for testing.

2. **Version Consistency**: All Solana SDK dependencies now use the same version (2.1) to avoid potential compatibility issues.

3. **Workspace Management**: All packages now properly use workspace-managed dependencies, making version updates easier and more consistent.

4. **Optional Dependencies**: Test-only dependencies are properly marked as optional and gated behind the `test-utils` feature.

These changes ensure that the Solana packages follow the same patterns and standards as the existing Ethereum packages in the workspace.
