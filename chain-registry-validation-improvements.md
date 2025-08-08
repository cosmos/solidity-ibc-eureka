# Chain Registry Validation Improvements

Based on the analysis of the cosmos/chain-registry repository, here are the improvements needed to ensure all fields mentioned by Cordt are automatically validated.

## Fields Analysis

### 1. Chain ID ✅ VALIDATED
- **Schema validation**: Required field in chain.schema.json
- **Uniqueness check**: validate_data.mjs checks for duplicate chain IDs
- **Location**: [.github/workflows/utility/validate_data.mjs#L70-L78](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs#L70-L78)

### 2. Chain Name ✅ VALIDATED
- **Schema validation**: Required field with pattern [a-z0-9]+
- **Directory validation**: Ensures chain_name matches directory name
- **Location**: validate_data.mjs

### 3. RPC ⚠️ PARTIALLY VALIDATED
- **Schema validation**: ✅ Structure validated
- **Endpoint testing**: ✅ But only for whitelisted providers
- **Location**: [.github/workflows/tests/apis.py#L97-L98](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/tests/apis.py#L97-L98)

### 4. REST ⚠️ PARTIALLY VALIDATED
- **Schema validation**: ✅ Structure validated
- **Endpoint testing**: ✅ But only for whitelisted providers
- **Location**: [.github/workflows/tests/apis.py#L99-L100](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/tests/apis.py#L99-L100)

### 5. GRPC ❌ NOT VALIDATED
- **Schema validation**: ✅ Structure defined
- **Endpoint testing**: ❌ Not implemented

### 6. EVM-RPC ❌ NOT VALIDATED
- **Schema validation**: ✅ Structure defined as "evm-http-jsonrpc"
- **Endpoint testing**: ❌ Not implemented

### 7. Address Prefix ✅ VALIDATED
- **Schema validation**: ✅ bech32_prefix field
- **SLIP-173 check**: ❌ Not validated against registry

### 8. Base Denom ✅ VALIDATED
- **Schema validation**: ✅ Required in assetlist
- **Cross-reference**: ✅ Checks fee/staking tokens exist
- **Location**: [.github/workflows/utility/validate_data.mjs#L93-L119](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs#L93-L119)

### 9. Cointype ⚠️ PARTIALLY VALIDATED
- **Schema validation**: ✅ slip44 field
- **Existence check**: ✅ For cosmos chains
- **SLIP-44 validation**: ❌ Not checked against registry

### 10. Native Token Decimals ✅ VALIDATED
- **Schema validation**: ✅ Through denom_units[].exponent
- **Base unit check**: ✅ Ensures exponent 0 exists
- **Location**: [.github/workflows/utility/validate_data.mjs#L121-L174](https://github.com/cosmos/chain-registry/blob/master/.github/workflows/utility/validate_data.mjs#L121-L174)

### 11. Block Explorer URL ❌ NOT VALIDATED
- **Schema validation**: ✅ Structure defined
- **URL testing**: ❌ Not implemented

### 12. Mainnet/Testnet ✅ VALIDATED
- **Schema validation**: ✅ network_type enum field
- **Values**: Must be "mainnet", "testnet", or "devnet"

## Recommended Improvements

### 1. Extend Endpoint Testing
Create enhanced testing that includes GRPC and EVM-RPC endpoints without whitelist limitations.

### 2. Add Registry Validation
Validate slip44 against SLIP-44 registry and bech32_prefix against SLIP-173.

### 3. Test Block Explorers
Add URL accessibility tests for block explorer endpoints.

### 4. Remove Whitelist Default
Make endpoint testing comprehensive by default, with whitelist as opt-in.

## Implementation Status
- Documentation created: ✅
- GRPC testing implementation: Attempted
- EVM-RPC testing implementation: Attempted
- Enhanced validation script: Attempted
- Workflow improvements: Proposed
