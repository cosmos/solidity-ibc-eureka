# Chain Registry Validation Analysis

This document analyzes the validation status of fields mentioned by Cordt in the cosmos/chain-registry repository.

## Fields to Validate

The fields mentioned by Cordt are:
- Chain ID
- Chain Name
- RPC
- REST
- GRPC
- EVM-RPC
- Address Prefix
- Base Denom
- Cointype
- Native Token Decimals
- Block Explorer URL
- Mainnet/Testnet

## Current Validation Status

### 1. Chain ID ✅ VALIDATED

**How it's tested:**
- **Schema validation**: Required field in `chain.schema.json` (line 46-49)
- **Uniqueness check**: `validate_data.mjs` checks for duplicate chain IDs across all chains
  - Location: [.github/workflows/utility/validate_data.mjs#L70-L78](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs#L70-L78)
  - Function: `checkChainIdConflict()`

### 2. Chain Name ✅ VALIDATED

**How it's tested:**
- **Schema validation**: Required field in `chain.schema.json` (line 14-18)
- **Format validation**: Must match pattern `[a-z0-9]+`
- **Directory match check**: `validate_data.mjs` ensures chain_name matches the directory name
  - Location: [.github/workflows/utility/validate_data.mjs](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs)
  - Function: `checkChainNameMatchDirectory()`

### 3. RPC Endpoints ⚠️ PARTIALLY VALIDATED

**How it's tested:**
- **Schema validation**: Structure validated in `chain.schema.json` (line 336-341)
- **Endpoint testing**: `test_endpoints.yml` workflow tests RPC endpoints for liveness
  - Location: [.github/workflows/tests/apis.py#L97-L98](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/tests/apis.py#L97-L98)
  - Tests: `/status` endpoint with 2-second timeout
  - **Limitation**: Only tests whitelisted providers by default

### 4. REST Endpoints ⚠️ PARTIALLY VALIDATED

**How it's tested:**
- **Schema validation**: Structure validated in `chain.schema.json` (line 342-346)
- **Endpoint testing**: `test_endpoints.yml` workflow tests REST endpoints
  - Location: [.github/workflows/tests/apis.py#L99-L100](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/tests/apis.py#L99-L100)
  - Tests: `/cosmos/base/tendermint/v1beta1/syncing` endpoint
  - **Limitation**: Only tests whitelisted providers by default

### 5. GRPC Endpoints ❌ NOT VALIDATED

**Current status:**
- **Schema validation**: Structure defined in `chain.schema.json` (line 348-352)
- **No endpoint testing**: The `test_endpoints.yml` workflow only tests RPC and REST, not GRPC

### 6. EVM-RPC Endpoints ❌ NOT VALIDATED

**Current status:**
- **Schema validation**: Structure defined in `chain.schema.json` (line 366-370) as `evm-http-jsonrpc`
- **No endpoint testing**: Not included in the current testing workflow

### 7. Address Prefix (bech32_prefix) ✅ VALIDATED

**How it's tested:**
- **Schema validation**: Defined in `chain.schema.json` (line 70-74)
- **Format requirements**: Must be a non-empty string
- **Note**: Should be registered with SLIP-0173

### 8. Base Denom ✅ VALIDATED

**How it's tested:**
- **Schema validation**: Required in `assetlist.schema.json` (line 88-92)
- **Cross-reference validation**: `validate_data.mjs` checks that fee tokens and staking tokens exist in assetlist
  - Location: [.github/workflows/utility/validate_data.mjs#L93-L119](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs#L93-L119)
  - Functions: `checkFeeTokensAreRegistered()`, `checkStakingTokensAreRegistered()`

### 9. Cointype (slip44) ⚠️ PARTIALLY VALIDATED

**How it's tested:**
- **Schema validation**: Defined in `chain.schema.json` (line 129-131)
- **Existence check**: `validate_data.mjs` ensures cosmos chains have slip44 defined
  - Location: [.github/workflows/utility/validate_data.mjs#L80-L91](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs#L80-L91)
  - **Limitation**: Only checks existence, not validity of the value

### 10. Native Token Decimals ✅ VALIDATED

**How it's tested:**
- **Schema validation**: In `assetlist.schema.json`, validated through `denom_units[].exponent`
- **Denom unit validation**: `validate_data.mjs` checks denom_units consistency
  - Location: [.github/workflows/utility/validate_data.mjs#L121-L174](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs#L121-L174)
  - Ensures base unit has exponent 0

### 11. Block Explorer URL ❌ NOT VALIDATED

**Current status:**
- **Schema validation**: Structure defined in `chain.schema.json` (explorers section, line 375-379)
- **No URL validation**: URLs are not tested for validity or accessibility

### 12. Mainnet/Testnet Status ✅ VALIDATED

**How it's tested:**
- **Schema validation**: `network_type` field in `chain.schema.json` (line 67-69)
- **Enum validation**: Must be one of: "mainnet", "testnet", "devnet"

## Summary Table

| Field | Schema Validation | Runtime Validation | Notes |
|-------|------------------|-------------------|-------|
| Chain ID | ✅ | ✅ | Uniqueness checked |
| Chain Name | ✅ | ✅ | Directory match checked |
| RPC | ✅ | ⚠️ | Only whitelisted providers tested |
| REST | ✅ | ⚠️ | Only whitelisted providers tested |
| GRPC | ✅ | ❌ | No endpoint testing |
| EVM-RPC | ✅ | ❌ | No endpoint testing |
| Address Prefix | ✅ | ❌ | No SLIP-173 validation |
| Base Denom | ✅ | ✅ | Cross-referenced with chain.json |
| Cointype | ✅ | ⚠️ | Only existence checked |
| Native Token Decimals | ✅ | ✅ | Exponent validation |
| Block Explorer URL | ✅ | ❌ | No URL validation |
| Mainnet/Testnet | ✅ | ✅ | Enum validation |

## Recommendations for Improvements

1. **Extend endpoint testing** to include GRPC and EVM-RPC endpoints
2. **Remove whitelist limitation** or make it configurable for comprehensive testing
3. **Add block explorer URL validation** to check if URLs are accessible
4. **Validate slip44/cointype values** against official SLIP-44 registry
5. **Add bech32_prefix validation** against SLIP-173 registry
6. **Implement comprehensive endpoint health checks** beyond basic connectivity