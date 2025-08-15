# ADR: Solana IBC Contract Upgradability

**Status**: Proposed
**Date**: 2025-08-15

## Context

The Solana IBC implementation requires a robust upgradability mechanism to support protocol evolution, security patches, and feature additions. Unlike Ethereum's UUPS proxy pattern, Solana offers different upgrade mechanisms with unique trade-offs around security, gas efficiency, and implementation complexity.

### Current State

**Ethereum Implementation:**
- Uses OpenZeppelin's UUPS (Universal Upgradeable Proxy Standard)
- AccessManager for role-based permissions with timelock
- Separate upgrade roles for different contract types
- Beacon proxies for IBC apps (escrow, IBCERC20)

**Solana Implementation:**
- Currently uses standard Anchor programs (non-upgradeable)
- Authority-based access control for admin functions
- No upgrade mechanism implemented yet

## Decision

We will implement a **hybrid upgrade strategy** combining Solana's native BPF upgradeable loader with versioned state management and migration capabilities.

### Architecture Overview

```
BPF Upgradeable Programs:
- ICS26 Router Program
  - Upgrade Authority: Multisig/Timelock
  - State Version: tracked in RouterState PDA
  - Migration Instructions: conditional execution

- ICS07 Tendermint Light Client
  - Upgrade Authority: Same as Router
  - State Version: in ClientState PDA
  - Backwards Compatible Updates
```

### Implementation Strategy

#### 1. BPF Upgradeable Loader

**Deploy programs as upgradeable:**
```bash
anchor deploy --program-keypair <keypair> --upgrade-authority <authority>
```

**Benefits:**
- Native Solana support, well-tested
- Atomic upgrades without state migration during upgrade
- Program data and executable separate
- Can freeze programs permanently if needed

**Limitations:**
- Single upgrade authority (mitigated with multisig)
- No built-in timelock (implement via Squads/Realms)
- Requires careful state compatibility

#### 2. State Versioning

```rust
#[account]
#[derive(InitSpace)]
pub struct RouterState {
    /// State version for migrations
    pub version: u8,
    /// Authority that can perform restricted operations
    pub authority: Pubkey,
    /// Upgrade authority (can be different from admin)
    pub upgrade_authority: Pubkey,
    /// Optional timelock for upgrades (unix timestamp)
    pub upgrade_timelock: Option<i64>,
}
```

#### 3. Migration Pattern

```rust
pub fn migrate_v1_to_v2(ctx: Context<Migrate>) -> Result<()> {
    let state = &mut ctx.accounts.router_state;

    require!(state.version == 1, ErrorCode::InvalidStateVersion);
    require!(
        ctx.accounts.authority.key() == state.upgrade_authority,
        ErrorCode::UnauthorizedUpgrade
    );

    // Check timelock if set
    if let Some(timelock) = state.upgrade_timelock {
        require!(
            Clock::get()?.unix_timestamp >= timelock,
            ErrorCode::TimelockNotExpired
        );
    }

    // Perform migration logic
    // ... migrate PDAs, update state format, etc ...

    state.version = 2;
    Ok(())
}
```

#### 4. Access Control Comparison

| Aspect | Ethereum | Solana (Proposed) |
|--------|----------|-------------------|
| Upgrade Authority | AccessManager with roles | Multisig via Squads/Realms |
| Timelock | Built into AccessManager | External governance program |
| Emergency Freeze | PAUSE_ROLE | Set upgrade authority to None |
| Role Granularity | Multiple upgrade roles | Single upgrade authority per program |
| Upgrade Process | upgradeToAndCall() | anchor upgrade + migrate instruction |

### Security Considerations

#### 1. Upgrade Authority Management

**Initial Setup:**
```bash
# Deploy with temporary authority
anchor deploy --upgrade-authority <deployer_keypair>

# After deployment, transfer to multisig
solana program set-upgrade-authority <program_id> \
    --upgrade-authority <deployer_keypair> \
    --new-upgrade-authority <multisig_address>
```

**Recommended Configuration:**
- 3-of-5 multisig for routine upgrades
- 5-of-7 for critical infrastructure
- Timelock period: 48-72 hours for non-emergency
- Emergency path: higher threshold, shorter timelock

#### 2. State Compatibility Rules

**Allowed Changes:**
- Adding new fields at the end of accounts
- Adding new instructions
- Modifying instruction logic (with care)
- Adding new PDA types

**Forbidden Changes:**
- Removing or reordering existing account fields
- Changing field types or sizes
- Modifying PDA seed derivation
- Breaking existing instruction signatures

#### 3. Rollback Strategy

Unlike Ethereum where rollbacks are complex, Solana allows:
```bash
# Save current program buffer
solana program dump <program_id> backup.so

# If upgrade fails, redeploy previous version
solana program deploy backup.so --program-id <program_id>
```

### Migration Path for Current Implementation

1. **Phase 1: Make Programs Upgradeable**
   - Redeploy existing programs with upgrade authority
   - Add version field to state accounts
   - No logic changes initially

2. **Phase 2: Implement Governance**
   - Deploy multisig (Squads Protocol recommended)
   - Transfer upgrade authority to multisig
   - Document upgrade procedures

3. **Phase 3: Add Migration Support**
   - Implement migration instructions
   - Add compatibility checks
   - Test on devnet with state migrations

### Cost Analysis

| Operation | Ethereum (Gas) | Solana (SOL) |
|-----------|---------------|--------------|
| Deploy Upgradeable | ~3M gas | ~2-3 SOL |
| Upgrade Program | ~50k gas | ~0.01 SOL |
| State Migration | ~100k-1M gas | ~0.001 SOL per account |
| Authority Transfer | ~30k gas | ~0.000005 SOL |

### Testing Strategy

```rust
#[cfg(test)]
mod upgrade_tests {
    use super::*;

    #[test]
    fn test_upgrade_authority() {
        // Test authority validation
    }

    #[test]
    fn test_state_migration() {
        // Test v1 -> v2 migration
    }

    #[test]
    fn test_backwards_compatibility() {
        // Ensure old transactions still work
    }
}
```

## Alternatives Considered

### 1. Immutable Programs with Proxy Pattern
- Deploy new program versions at new addresses
- Router maintains mapping of versions
- **Rejected**: Complex state migration, higher operational overhead

### 2. Versioned Program Accounts
- Multiple program versions coexist
- Clients choose which version to call
- **Rejected**: Increased complexity, potential for fragmentation

### 3. Full State Migration on Upgrade
- Close all PDAs and recreate with new program
- **Rejected**: Extremely expensive, service downtime

## Consequences

### Positive
- Seamless upgrades without service interruption
- Lower upgrade costs than Ethereum
- Native Solana tooling support
- Clear rollback path
- Flexibility for future protocol changes

### Negative
- Less granular access control than Ethereum
- Requires external governance infrastructure
- State compatibility constraints
- Additional complexity in migration instructions

### Neutral
- Different operational procedures from Ethereum
- Requires Solana-specific expertise
- Dependency on external multisig solutions

