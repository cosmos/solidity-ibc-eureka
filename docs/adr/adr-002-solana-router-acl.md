# ADR-002: Solana Router Access Control List (ACL) Design

## Context
The Solana ICS26 router requires role-based access control (RBAC) to enable multi-party operation and achieve feature parity with the Ethereum implementation, which uses OpenZeppelin's AccessManager. This ADR proposes an ACL design that respects Solana's account model constraints while providing granular permission management.

## Decision

### Design Principles
1. **Compatibility**: Maintain backwards compatibility with existing single-authority model during migration
2. **Rent Efficiency**: Minimize account rent and computation costs typical in Solana programs
3. **Feature Parity**: Support similar roles to Ethereum implementation where applicable
4. **Solana Native**: Leverage Program Derived Addresses (PDAs) for deterministic account derivation

### Proposed Architecture

#### 1. Role Definitions
Based on Ethereum's `IBCRolesLib.sol`, adapt the following roles for Solana:

- **Admin** - Manage roles and upgrade contracts (maps to `ADMIN_ROLE`)
- **Relayer** - Relay packets (recv, ack, timeout) and update clients (maps to `RELAYER_ROLE`)
- **IdCustomizer** - Add IBC apps and clients with custom IDs (maps to `ID_CUSTOMIZER_ROLE`)
- **Pauser** - Pause operations for ICS20Transfer (maps to `PAUSER_ROLE`)
- **Unpauser** - Unpause operations for ICS20Transfer (maps to `UNPAUSER_ROLE`)
- **ClientMigrator** - Migrate and upgrade light clients (per-client role in Ethereum)

Roles are represented as string identifiers rather than enums to allow extensibility without program upgrades:
```rust
pub type Role = String; // e.g., "admin", "relayer", "id_customizer"
```

**Note**: Ethereum's `DELEGATE_SENDER_ROLE`, `RATE_LIMITER_ROLE`, and `ERC20_CUSTOMIZER_ROLE` are ICS20Transfer-specific and excluded from router ACL.

#### 2. Account Structure

##### AclState Account (PDA)
```rust
#[account]
pub struct AclState {
    pub router: Pubkey,      // Associated router
    pub admin: Pubkey,       // Admin who manages roles
    pub pending_admin: Option<Pubkey>, // For 2-step admin transfer
    pub bump: u8,            // PDA bump seed
}
// Seeds: [b"acl_state", router_pubkey]
```

##### RoleAssignment Account (PDA)
```rust
#[account]
pub struct RoleAssignment {
    pub acl_state: Pubkey,   // Parent ACL state
    pub grantee: Pubkey,     // Address with the role
    pub role: String,        // Role identifier (e.g., "admin", "relayer")
    pub granted_at: i64,     // Unix timestamp of grant
    pub bump: u8,            // PDA bump seed
}
// Seeds: [b"role_assignment", acl_state, grantee.as_ref(), role.as_bytes()]
```

##### Updated RouterState
```rust
#[account]
pub struct RouterState {
    pub authority: Pubkey,    // Legacy field for backwards compatibility
    pub acl_state: Pubkey,    // ACL state account
    pub acl_enabled: bool,    // Toggle between legacy and ACL mode
}
```

#### 3. Core Instructions

##### Role Management
- `initialize_acl` - Convert router from single-authority to ACL mode
- `grant_role` - Admin grants role to an address
- `grant_roles` - Admin grants multiple roles atomically
- `revoke_role` - Admin revokes role from an address
- `revoke_all_roles` - Emergency revocation of all roles for a grantee
- `propose_admin_transfer` - Initiate 2-step admin transfer
- `accept_admin_transfer` - Accept admin role transfer
- `migrate_to_acl` - One-time migration from authority to ACL

#### 4. Permission Model

##### Role Verification
```rust
pub enum AclError {
    RoleNotFound,
    InsufficientPermissions,
    InvalidRoleAccount,
    AdminTransferPending,
    NotPendingAdmin,
}
```

Role checks require passing the RoleAssignment PDA in remaining accounts to avoid CPI overhead.

##### Role Hierarchy
- Admin role does NOT implicitly grant all permissions (separation of duties)
- Each role must be explicitly checked in relevant instructions
- No automatic permission inheritance between roles

### Security Considerations

1. **Role Enumeration**: Support efficient querying via getProgramAccounts filters
2. **Emergency Recovery**: Include program-level emergency pause separate from admin role
3. **Upgrade Persistence**: Role assignments persist through program upgrades via PDAs
4. **Audit Trail**: Emit events for all role changes for off-chain monitoring
5. **Two-Step Admin Transfer**: Prevent accidental admin lockout with pending/accept pattern

### Cost Analysis

#### Storage Costs
- `AclState`: ~0.0016 SOL
- `RoleAssignment`: ~0.0019 SOL per assignment
- Typical setup (5 roles): ~0.01 SOL total

#### Operational Costs
- Orders of magnitude cheaper than Ethereum equivalent operations
- Minimal compute overhead for permission checks
- No ongoing gas costs unlike Ethereum

### Trade-offs

#### Pros
- Granular permissions with multi-party support
- Clear migration path from single-authority
- On-chain audit trail with timestamps
- **Extensible role system** - New roles can be added without program upgrades
- Significantly lower costs than Ethereum

#### Cons
- Additional account rent (~0.01 SOL for typical setup)
- Increased transaction size for role checks
- More complex than single-authority model
- Requires off-chain indexing for role enumeration
- No built-in timelock (requires separate program)
- **String-based roles** use more storage than enums

### Alternative Approaches Considered

1. **Fixed Array in RouterState**: More efficient for small role sets but lacks flexibility
2. **Centralized ACL Program**: Rejected due to CPI overhead and complexity
3. **Merkle Tree Roles**: Rejected due to off-chain proof requirements

## Feature Comparison with Ethereum

### Supported Features
✅ Role-based access control with multiple roles
✅ Admin role management (grant/revoke)
✅ Relayer permissions for packet relay
✅ ID customization for ports and clients
✅ Pause/unpause functionality
✅ Client migration permissions
✅ Backwards compatibility
✅ Batch role operations
✅ Two-step admin transfer

### Not Supported / Different Approach
❌ Time-delayed operations (requires separate timelock program)
❌ Guardian role for canceling operations
❌ Per-function granularity (Solana uses per-instruction)
❌ Automatic permission modifiers
❌ ICS20-specific roles (separate ADR needed)

## Consequences

### Positive
- Achieves core RBAC parity with Ethereum
- Enables secure multi-party router operation
- Maintains Solana best practices
- Provides extensible foundation for future enhancements

### Negative
- Increases complexity for operators
- Requires careful migration planning
- Lacks advanced timelock features without additional infrastructure

### Neutral
- Requires relayer software updates
- Documentation and tooling updates needed
- Separate design needed for ICS20Transfer ACL

## References
- [Ethereum IBCRolesLib Implementation](../../contracts/utils/IBCRolesLib.sol)
- [OpenZeppelin AccessManager](https://docs.openzeppelin.com/contracts/5.x/api/access#AccessManager)
- [Solana PDA Documentation](https://docs.solana.com/developing/programming-model/calling-between-programs#program-derived-addresses)
