# ADR: Solana IBC Contract Upgradability

**Status**: Proposed
**Date**: 2025-07-15

## Context

The Solana IBC implementation requires a robust upgradability mechanism to support protocol evolution, security patches, and feature additions. Solana's BPF upgradeable loader provides native upgrade support, but requires careful design around state management, migration strategies, and governance integration.

## Decision

Implement upgradability using Solana's native BPF upgradeable loader with:
- Reserved space for account evolution
- Versioned state with migration instructions
- Multisig governance via Squads Protocol
- Discriminator stability guarantees

## Architecture

### 1. State Design with Reserved Space

```rust
#[account]
pub struct RouterState {
    pub version: u8,
    pub authority: Pubkey,
    pub upgrade_authority: Pubkey,
    pub paused: bool,
    pub migration_in_progress: bool,
    pub _reserved: [u8; 256], // Critical: Reserve space for future fields
}

#[account]
pub struct ClientState {
    pub version: u8,
    pub client_type: [u8; 32],
    pub frozen: bool,
    pub latest_height: Height,
    pub _reserved: [u8; 512], // More space for complex client evolution
}
```

### 2. Migration State Machine

```rust
pub enum MigrationState {
    None,
    Pending { target_version: u8, deadline: i64 },
    InProgress { from_version: u8, to_version: u8 },
    Complete,
}

#[account]
pub struct MigrationStatus {
    pub state: MigrationState,
    pub migrated_accounts: u64,
    pub total_accounts: u64,
    pub last_migrated_key: Option<Pubkey>,
}
```

### 3. Version-Aware Instructions

```rust
impl RouterState {
    pub fn check_version(&self, min_version: u8) -> Result<()> {
        require!(
            self.version >= min_version,
            ErrorCode::IncompatibleVersion
        );
        require!(
            !self.migration_in_progress,
            ErrorCode::MigrationInProgress
        );
        Ok(())
    }
}

// Every instruction checks compatibility
pub fn recv_packet(ctx: Context<RecvPacket>, packet: Packet) -> Result<()> {
    ctx.accounts.router_state.check_version(CURRENT_VERSION)?;
    // ... instruction logic
}
```

### 4. Deployment and Upgrade Process

```bash
# Initial deployment (upgradeable by default)
solana program deploy target/deploy/router.so \
    --upgrade-authority deployer.json

# Transfer to multisig (Squads)
squads-cli create-program-upgrade \
    --program <PROGRAM_ID> \
    --multisig <SQUADS_MULTISIG> \
    --threshold 3

# Upgrade via multisig
squads-cli propose-program-upgrade \
    --program <PROGRAM_ID> \
    --buffer <NEW_PROGRAM_BUFFER> \
    --multisig <SQUADS_MULTISIG>
```

### 5. Migration Instructions

```rust
/// Start migration process
pub fn begin_migration(ctx: Context<BeginMigration>, target_version: u8) -> Result<()> {
    let router = &mut ctx.accounts.router_state;

    require!(
        ctx.accounts.authority.key() == router.upgrade_authority,
        ErrorCode::Unauthorized
    );

    router.migration_in_progress = true;

    ctx.accounts.migration_status.state = MigrationState::Pending {
        target_version,
        deadline: Clock::get()?.unix_timestamp + 86400, // 24 hours
    };

    emit!(MigrationStarted {
        from_version: router.version,
        to_version: target_version,
    });

    Ok(())
}

/// Migrate accounts in batches
pub fn migrate_batch(
    ctx: Context<MigrateBatch>,
    accounts: Vec<Pubkey>,
) -> Result<()> {
    let status = &mut ctx.accounts.migration_status;

    for account_key in accounts {
        // Load account
        let account_info = ctx.remaining_accounts
            .iter()
            .find(|a| a.key() == &account_key)
            .ok_or(ErrorCode::AccountNotFound)?;

        // Migrate based on type
        migrate_account(account_info, status)?;

        status.migrated_accounts += 1;
        status.last_migrated_key = Some(account_key);
    }

    // Check if complete
    if status.migrated_accounts >= status.total_accounts {
        complete_migration(ctx)?;
    }

    Ok(())
}

fn migrate_account(account: &AccountInfo, status: &MigrationStatus) -> Result<()> {
    // Version-specific migration logic
    match status.state {
        MigrationState::InProgress { from_version: 1, to_version: 2 } => {
            // V1 -> V2 migration
            // Carefully deserialize old format, upgrade, serialize new format
        },
        _ => return Err(ErrorCode::InvalidMigration),
    }
    Ok(())
}
```

### 6. Discriminator Stability

```rust
// Use explicit discriminators to prevent Anchor changes
#[program]
pub mod router {
    use super::*;

    // Explicitly declare discriminators for stability
    declare_id!("RouterV1111111111111111111111111111111111111");

    #[instruction(discriminator = [1, 2, 3, 4, 5, 6, 7, 8])]
    pub fn recv_packet(ctx: Context<RecvPacket>, packet: Packet) -> Result<()> {
        // Discriminator won't change across Anchor versions
    }
}
```

### 7. Backward Compatibility Layer

```rust
pub fn handle_legacy_instruction(
    ctx: Context<LegacyHandler>,
    instruction_data: Vec<u8>,
) -> Result<()> {
    let version = ctx.accounts.router_state.version;

    match version {
        1 => handle_v1_instruction(ctx, instruction_data),
        2 => handle_v2_instruction(ctx, instruction_data),
        _ => Err(ErrorCode::UnsupportedVersion.into()),
    }
}
```

## Governance Integration

### What is Squads Protocol
Squads Protocol is Solana's leading multisig wallet infrastructure, serving as the standard for secure program upgrade management and treasury operations. It enables teams to collectively manage on-chain assets and program authorities through multi-signature wallets requiring M-of-N approvals.
Key capabilities for IBC:

- Program Upgrade Management: Securely control upgrade authorities with multiple signers
- Transaction Proposals: Create, review, and approve complex operations before execution
- Role-Based Access: Define proposers, voters, and executors with granular permissions
- Audit Trail: On-chain record of all proposals and approvals

Why Squads for IBC Router:

- Battle-tested security (manages billions in TVL across Solana)
- Industry standard (used by Jupiter, Drift, Marinade)
- No custom multisig code needed
- Clean UI at app.squads.so for non-technical operators
- Active support and regular updates

### Squads Protocol Setup

```typescript
// 1. Create multisig
const multisig = await Squads.create({
    members: [
        { address: key1, permissions: Permission.PROPOSER },
        { address: key2, permissions: Permission.VOTER },
        { address: key3, permissions: Permission.VOTER },
        // ... more members
    ],
    threshold: 3,
});

// 2. Create program upgrade proposal
const proposal = await multisig.createProgramUpgrade({
    programId: ROUTER_PROGRAM_ID,
    bufferAccount: newProgramBuffer,
    spillAccount: upgradeAuthority,
});

// 3. Vote and execute
await proposal.vote(memberKeypair);
if (proposal.hasReachedThreshold()) {
    await proposal.execute();
}
```

### Emergency Procedures

```rust
#[account]
pub struct EmergencyState {
    pub paused: bool,
    pub emergency_authority: Pubkey,
    pub pause_timestamp: i64,
    pub auto_unpause_after: i64, // Auto-unpause after duration
}

pub fn emergency_pause(ctx: Context<EmergencyPause>) -> Result<()> {
    let emergency = &mut ctx.accounts.emergency_state;

    require!(
        ctx.accounts.authority.key() == emergency.emergency_authority,
        ErrorCode::Unauthorized
    );

    emergency.paused = true;
    emergency.pause_timestamp = Clock::get()?.unix_timestamp;

    emit!(EmergencyPauseActivated {
        authority: ctx.accounts.authority.key(),
        timestamp: emergency.pause_timestamp,
    });

    Ok(())
}
```

## Account Size Management

### Strategy for Field Addition

```rust
// Version 1
#[account]
pub struct RouterState {
    pub version: u8,           // 1 byte
    pub authority: Pubkey,     // 32 bytes
    pub _reserved: [u8; 256],  // 256 bytes
}

// Version 2 - Using reserved space
#[account]
pub struct RouterState {
    pub version: u8,           // 1 byte
    pub authority: Pubkey,     // 32 bytes
    pub new_field: u64,        // 8 bytes (from reserved)
    pub _reserved: [u8; 248],  // 248 bytes (reduced)
}
```

### Handling Fixed Account Sizes

For accounts that need size increases:

```rust
pub fn migrate_to_larger_account(
    ctx: Context<MigrateAccount>,
) -> Result<()> {
    let old_account = &ctx.accounts.old_account;
    let new_account = &mut ctx.accounts.new_account;

    // Copy data
    new_account.set_inner(old_account.clone());

    // Mark old account for cleanup
    old_account.mark_obsolete()?;

    // Update references in router
    ctx.accounts.router.update_reference(
        old_account.key(),
        new_account.key(),
    )?;

    Ok(())
}
```

## Testing Strategy

### Upgrade Simulation Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_upgrade_with_migration() {
        let mut test = ProgramTest::new(
            "router",
            ROUTER_PROGRAM_ID,
            processor!(process_instruction),
        );

        // Deploy V1
        let mut context = test.start_with_context().await;

        // Create state with V1 schema
        create_v1_state(&mut context).await;

        // Simulate upgrade to V2
        upgrade_program(&mut context, "router_v2.so").await;

        // Run migration
        begin_migration(&mut context, 2).await;
        migrate_all_accounts(&mut context).await;

        // Verify V2 functionality
        test_v2_instructions(&mut context).await;

        // Verify backward compatibility
        test_v1_client_compatibility(&mut context).await;
    }
}
```

## Rollback Strategy

### Automatic Rollback on Failure

```rust
pub fn auto_rollback(ctx: Context<AutoRollback>) -> Result<()> {
    let status = &ctx.accounts.migration_status;

    match status.state {
        MigrationState::Pending { deadline, .. } => {
            if Clock::get()?.unix_timestamp > deadline {
                // Migration timeout - rollback
                revert_upgrade(ctx)?;
            }
        },
        MigrationState::InProgress { .. } => {
            if status.consecutive_failures > MAX_FAILURES {
                revert_upgrade(ctx)?;
            }
        },
        _ => {}
    }

    Ok(())
}
```

## Cost Analysis

| Operation | Cost (SOL) | Notes |
|-----------|------------|-------|
| Deploy Upgradeable | 1-2 | Depends on program size |
| Program Upgrade | 0.01 | Buffer account rent |
| Migration per Account | 0.000005 | Transaction fee only |
| Authority Transfer | 0.000005 | Single transaction |
| Emergency Pause | 0.000005 | Single transaction |

## Security Considerations

### Multi-Stage Validation

1. **Pre-upgrade**: Validate new program on devnet
2. **Staging**: Deploy to canary accounts
3. **Migration**: Gradual rollout with monitoring
4. **Post-upgrade**: Verification period before finalization

### Access Control Matrix

| Action | Required Authority | Timelock |
|--------|-------------------|----------|
| Program Upgrade | 3-of-5 Multisig | 48 hours |
| Emergency Pause | 2-of-3 Emergency | None |
| Migration Start | Upgrade Authority | 24 hours |
| Rollback | 2-of-5 Multisig | None |

## Trade-offs

### Pros
- Native Solana upgrade mechanism
- No proxy overhead
- Clear migration path
- Automatic rollback capability
- Reserved space for evolution

### Cons
- Fixed account sizes require planning
- Complex migration for large state changes
- No per-function upgrade granularity
- Requires external governance infrastructure

## Consequences

### Positive
- Future-proof design with reserved space
- Safe migration with rollback capability
- Clear governance model
- Minimal performance overhead

### Negative
- Increased complexity in state management
- Requires careful version planning
- Migration can temporarily impact performance

### Neutral
- Different from Ethereum upgrade patterns
- Requires Solana-specific expertise
- Ongoing monitoring requirements
