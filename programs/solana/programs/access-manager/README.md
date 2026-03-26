# Access Manager

Role-based access control for Solana IBC programs. Mirrors Ethereum's OpenZeppelin `AccessManager` pattern, providing unified governance over both runtime operations and program upgrades.

## Overview

The access manager maintains a central registry of roles and their members. Every permissioned operation across Solana IBC programs (relaying, pausing, upgrading) delegates authorization to this single account. It also controls program upgrade authority through PDAs, enabling role-based upgrades without exposing raw keypairs.

## State

```
AccessManager PDA (seeds: ["access_manager"])
  roles:                       Vec<RoleData>                      -- role ID -> member list
  whitelisted_programs:        Vec<Pubkey>                        -- programs allowed to call admin-gated instructions via CPI (e.g. multisig)
  pending_authority_transfer:  Option<PendingAuthorityTransfer>   -- pending two-step upgrade authority transfer
```

Role IDs are opaque `u64` values defined in `solana-ibc-types::roles`. The access manager does not interpret them -- consuming programs define what each role means.

## Instructions

### `initialize`

Creates the `AccessManager` PDA and sets the initial admin. Only callable by the program's upgrade authority (deployer). Rejects CPI.

### `grant_role` / `revoke_role`

Adds or removes an account from a role. Requires `ADMIN_ROLE`. The last admin cannot be removed.

### `renounce_role`

Allows an account to remove itself from a role. Does not require admin authorization.

### `set_whitelisted_programs`

Replaces the list of programs allowed to invoke admin-gated instructions via CPI. Requires `ADMIN_ROLE`.

### `upgrade_program`

Upgrades a target program's bytecode via BPF Loader Upgradeable. The access manager's PDA acts as the upgrade authority, signing the BPF Loader `Upgrade` call via `invoke_signed`. Requires `ADMIN_ROLE`. Allows whitelisted CPI.

### `propose_upgrade_authority_transfer`

Proposes transferring a target program's BPF Loader upgrade authority from this access manager's PDA to a new address. Sets a pending transfer on the `AccessManager` state. Requires `ADMIN_ROLE`. Allows whitelisted CPI. Only one pending transfer at a time.

### `accept_upgrade_authority_transfer`

Accepts a pending upgrade authority transfer by executing the BPF Loader `SetAuthority` CPI. Must be signed by the proposed new authority. No CPI restriction (supports both keypair signers and multisig/PDA callers).

This operation is irreversible from this access manager's perspective -- once accepted, only the new authority can upgrade the target program.

### `cancel_upgrade_authority_transfer`

Cancels a pending upgrade authority transfer. Requires `ADMIN_ROLE`. Allows whitelisted CPI.

### `claim_upgrade_authority`

Claims upgrade authority from a source access manager that has proposed a transfer to this access manager's upgrade authority PDA. CPIs into the source AM's `accept_upgrade_authority_transfer` with this AM's PDA as signer. No admin authorization required -- PDA signing is the authorization.

## PDA Derivations

```
access_manager:    ["access_manager"]                              program: access_manager
upgrade_authority: ["upgrade_authority", target_program.as_ref()]   program: access_manager
program_data:      [target_program.as_ref()]                       program: BPF Loader Upgradeable
```

## Program Upgrade Flow

### Standard Upgrade via Access Manager

```mermaid
graph TB
    subgraph setup["① Setup (one-time)"]
        Deployer["Deployer"]
        PD_S["ProgramData\nauthority: deployer"]
        AM_S["AccessManager PDA"]

        Deployer -->|"1. deploy program"| PD_S
        Deployer -->|"2. initialize AM\ngrant ADMIN_ROLE"| AM_S
        Deployer -->|"3. set-upgrade-authority\ndeployer → UA PDA"| PD_S
    end

    subgraph upgrade["② Upgrade"]
        Deployer2["Deployer"]
        Admin["Admin (ADMIN_ROLE)"]
        Buffer["Buffer Account\nnew bytecode"]
        UPIX["upgrade_program()"]
        AM_U["AccessManager PDA"]
        UA_U["Upgrade Authority PDA"]
        PD_U["ProgramData\nauthority: UA PDA"]
        Target["Target Program"]

        Deployer2 -->|"1. write-buffer +\nset-buffer-authority → UA PDA"| Buffer
        Admin -->|"2. call"| UPIX
        UPIX -->|"3. require_admin"| AM_U
        UPIX -->|"4. invoke_signed"| UA_U
        UA_U -->|"5. BPFLoader::upgrade()"| PD_U
        Buffer -->|"bytecode source"| PD_U
        PD_U -->|"6. bytecode replaced"| Target
    end

    setup ~~~ upgrade

    style setup fill:#ffedd5,stroke:#ea580c,color:#7c2d12
    style upgrade fill:#d1fae5,stroke:#059669,color:#064e3b

    classDef actor fill:#fed7aa,stroke:#ea580c,color:#7c2d12
    classDef am fill:#c7d2fe,stroke:#4f46e5,color:#1e1b4b
    classDef pda fill:#fbcfe8,stroke:#db2777,color:#831843
    classDef ix fill:#fef08a,stroke:#ca8a04,color:#713f12
    classDef bpf fill:#e2e8f0,stroke:#475569,color:#1e293b
    classDef target fill:#a7f3d0,stroke:#059669,color:#064e3b

    class Deployer,Deployer2,Admin actor
    class AM_S,AM_U am
    class UA_U pda
    class UPIX ix
    class PD_S,PD_U,Buffer bpf
    class Target target
```

**Setup (one-time):**
1. Deploy programs with deployer keypair as upgrade authority
2. Initialize access manager, grant `ADMIN_ROLE`
3. Transfer each program's upgrade authority to the access manager's PDA via `solana program set-upgrade-authority`

**Upgrade flow:**
1. Write new bytecode to a buffer account
2. Set buffer authority to the access manager's upgrade authority PDA
3. Call `upgrade_program()` with an admin signer -- the PDA signs the BPF Loader CPI

### Authority Transfer (Two-Step Propose/Accept)

When migrating to a new access manager or transferring upgrade control, the transfer uses a two-step propose/accept pattern to prevent irreversible mistakes:

```mermaid
graph TB
    subgraph actors["Actors"]
        Admin["Admin (ADMIN_ROLE)"]
        NewAuth["New Authority\n(keypair or PDA)"]
    end

    subgraph am["Access Manager"]
        PROPOSE["propose_upgrade_authority_transfer()"]
        ACCEPT["accept_upgrade_authority_transfer()"]
        AM_State["AccessManager PDA\npending_authority_transfer"]
        UA_PDA["Upgrade Authority PDA\ncurrent authority"]
    end

    subgraph bpfloader["BPF Loader Upgradeable"]
        PD["ProgramData\nupgrade_authority"]
    end

    subgraph target["Target Program"]
        Program["Program Account"]
    end

    Admin -->|"1. propose transfer\n(target, new_authority)"| PROPOSE
    PROPOSE -->|"2. require_admin"| AM_State
    PROPOSE -->|"3. set pending"| AM_State
    NewAuth -->|"4. accept transfer\n(signer = new_authority)"| ACCEPT
    ACCEPT -->|"5. invoke_signed"| UA_PDA
    UA_PDA -->|"6. set_authority()\nold → new"| PD
    PD -.->|"authority now:\nNew Authority"| NewAuth
    NewAuth -->|"7. can upgrade"| Program

    style actors fill:#ffedd5,stroke:#ea580c,color:#7c2d12
    style am fill:#e0e7ff,stroke:#4f46e5,color:#1e1b4b
    style bpfloader fill:#f1f5f9,stroke:#475569,color:#1e293b
    style target fill:#d1fae5,stroke:#059669,color:#064e3b

    classDef actor fill:#fed7aa,stroke:#ea580c,color:#7c2d12
    classDef amNode fill:#c7d2fe,stroke:#4f46e5,color:#1e1b4b
    classDef pda fill:#fbcfe8,stroke:#db2777,color:#831843
    classDef ix fill:#fef08a,stroke:#ca8a04,color:#713f12
    classDef bpf fill:#e2e8f0,stroke:#475569,color:#1e293b
    classDef targetNode fill:#a7f3d0,stroke:#059669,color:#064e3b

    class Admin,NewAuth actor
    class AM_State amNode
    class UA_PDA pda
    class PROPOSE,ACCEPT ix
    class PD bpf
    class Program targetNode
```

The admin can also call `cancel_upgrade_authority_transfer` to abort a pending proposal before the new authority accepts.

### AM-to-AM Migration

Replacing one access manager instance (AM-A) with another (AM-B) requires migrating two independent control planes:

| Control plane | What it governs | Where it's stored | How to migrate |
|---|---|---|---|
| **Upgrade authority** | Who can replace program bytecode | BPF Loader's `ProgramData.upgrade_authority` | `propose` + `claim_upgrade_authority` (per managed program) |
| **Runtime roles** | Who can relay, pause, admin-gate operations | Each IBC program's state (e.g. `RouterState.access_manager`) | `set_access_manager` (per IBC program) |

These are fully independent -- migrating one does not affect the other.

#### Upgrade authority migration

AM-B's upgrade authority PDA must sign the accept transaction, but PDAs can only sign via `invoke_signed` from their owning program. The `claim_upgrade_authority` instruction solves this:

```mermaid
graph TB
    subgraph step1["① Propose (AM-A admin)"]
        Admin["AM-A Admin"]
        AMA_propose["AM-A:\npropose_upgrade_authority_transfer()"]
        AMA_state["AM-A State\npending: AM-B's PDA"]

        Admin -->|"propose transfer\n(target, AM-B's PDA)"| AMA_propose
        AMA_propose -->|"require_admin"| AMA_state
        AMA_propose -->|"set pending"| AMA_state
    end

    subgraph step2["② Claim (permissionless)"]
        Anyone["Anyone"]
        AMB_claim["AM-B:\nclaim_upgrade_authority()"]
        AMA_accept["AM-A:\naccept_upgrade_authority_transfer()"]
        AMA_clear["AM-A State\npending: None"]
        BPF["BPF Loader\nset_authority()"]
        PD["ProgramData\nauthority: AM-B's PDA"]

        Anyone -->|"call claim"| AMB_claim
        AMB_claim -->|"CPI with\nPDA signer"| AMA_accept
        AMA_accept -->|"validate pending\nmatches AM-B's PDA"| AMA_clear
        AMA_accept -->|"invoke_signed"| BPF
        BPF -->|"authority transferred"| PD
    end

    step1 ~~~ step2

    style step1 fill:#ffedd5,stroke:#ea580c,color:#7c2d12
    style step2 fill:#d1fae5,stroke:#059669,color:#064e3b

    classDef actor fill:#fed7aa,stroke:#ea580c,color:#7c2d12
    classDef amaNode fill:#c7d2fe,stroke:#4f46e5,color:#1e1b4b
    classDef ambNode fill:#fbcfe8,stroke:#db2777,color:#831843
    classDef ix fill:#fef08a,stroke:#ca8a04,color:#713f12
    classDef bpf fill:#e2e8f0,stroke:#475569,color:#1e293b

    class Admin,Anyone actor
    class AMA_propose,AMA_accept,AMA_state,AMA_clear amaNode
    class AMB_claim ambNode
    class BPF,PD bpf
```

Repeat for each managed program (ICS07, ICS26, GMP, etc.). No admin role is required on the claim side -- PDA signing is the authorization.

#### Runtime role migration

Each IBC program (ICS07, ICS26, GMP, attestation) stores an `access_manager: Pubkey` field in its state that points to the access manager it delegates role checks to. Calling `set_access_manager` on each program repoints it from AM-A to AM-B:

```mermaid
graph TB
    subgraph before["Before: roles delegated to AM-A"]
        ICS26_old["ICS26 Router\naccess_manager: AM-A"]
        AMA_roles["AM-A State\nroles, whitelist"]
        ICS26_old -.->|"role checks"| AMA_roles
    end

    subgraph migrate["set_access_manager (requires ADMIN_ROLE on current AM)"]
        Admin["Admin"]
        SetAM["set_access_manager(AM-B)"]
        Admin -->|"call"| SetAM
    end

    subgraph after["After: roles delegated to AM-B"]
        ICS26_new["ICS26 Router\naccess_manager: AM-B"]
        AMB_roles["AM-B State\nroles, whitelist"]
        ICS26_new -.->|"role checks"| AMB_roles
    end

    before ~~~ migrate
    migrate ~~~ after

    style before fill:#f1f5f9,stroke:#475569,color:#1e293b
    style migrate fill:#fef9c3,stroke:#ca8a04,color:#713f12
    style after fill:#d1fae5,stroke:#059669,color:#064e3b

    classDef actor fill:#fed7aa,stroke:#ea580c,color:#7c2d12
    classDef ix fill:#fef08a,stroke:#ca8a04,color:#713f12
    classDef oldNode fill:#e2e8f0,stroke:#475569,color:#1e293b
    classDef amaNode fill:#c7d2fe,stroke:#4f46e5,color:#1e1b4b
    classDef ambNode fill:#fbcfe8,stroke:#db2777,color:#831843
    classDef newNode fill:#a7f3d0,stroke:#059669,color:#064e3b

    class Admin actor
    class SetAM ix
    class ICS26_old oldNode
    class AMA_roles amaNode
    class AMB_roles ambNode
    class ICS26_new newNode
```

Repeat for each IBC program. IFT uses a different pattern (`admin: Pubkey` with two-step propose/accept transfer) and does not use `set_access_manager`.

> **Future improvement:** `set_access_manager` is currently a one-step operation. If an admin accidentally points it to a wrong or nonexistent AM address, the program becomes unrecoverable through normal admin operations -- all future admin-gated calls (including another `set_access_manager` to fix the mistake) would fail because `require_admin` reads roles from the now-invalid AM. A two-step propose/accept pattern (similar to upgrade authority transfer) would prevent this: the admin proposes a new AM, and then someone with `ADMIN_ROLE` on the *new* AM accepts, proving it is valid and operational before the switch takes effect.

#### Full migration checklist

Assuming AM-B is already deployed and initialized with its own admin:

1. **AM-A admin proposes upgrade authority transfers** -- call `propose_upgrade_authority_transfer` on AM-A for each managed program, specifying AM-B's upgrade authority PDA as the new authority
2. **Anyone claims on AM-B** -- call `claim_upgrade_authority` on AM-B for each program (permissionless, PDA signing is the authorization)
3. **Verify upgrade authority** -- confirm each program's `ProgramData.upgrade_authority` now points to AM-B's PDA
4. **Repoint runtime roles** -- call `set_access_manager` on each IBC program (ICS07, ICS26, GMP, attestation) to point at AM-B. This requires `ADMIN_ROLE` on whichever AM currently controls the program
5. **Verify runtime roles** -- confirm AM-B admin can perform role-gated operations (e.g. grant roles, relay packets)
6. **Migrate IFT admin** (if applicable) -- use IFT's `propose_admin_transfer` + `accept_admin_transfer`

After migration, AM-A has no remaining authority over any program. AM-B controls both bytecode upgrades and runtime roles.

## Security

#### CPI validation

`require_admin` checks the instructions sysvar to validate the caller. Direct calls and whitelisted CPI are allowed; unauthorized and nested CPI are rejected.

#### Sysvar address constraint

The instructions sysvar account has an `address` constraint preventing fake sysvar attacks (Wormhole-style).

#### Two-step authority transfer

Authority transfers require propose + accept, preventing irreversible mistakes from a single admin action.

#### Zero-address rejection

`propose_upgrade_authority_transfer` rejects `Pubkey::default()` to prevent irreversible lockout.

#### Self-transfer rejection

`propose_upgrade_authority_transfer` rejects transferring to the current upgrade authority PDA.

#### Last admin protection

The last admin cannot be removed via `revoke_role`.

#### Per-program PDA scoping

Upgrade authority PDAs include the target program ID in their seeds, preventing cross-program authority reuse.

## Testing

### Unit and Integration Tests

```bash
just build-solana access-manager
cargo test -p access-manager --lib --tests
```

The test suite includes Mollusk (SBF binary) unit tests and ProgramTest integration tests covering admin authorization, CPI rejection, fake sysvar attacks, wrong PDA derivation and zero-address rejection.

### E2E Tests

Tests are in `e2e/interchaintestv8/solana_upgrade_test.go`:
- `Test_ProgramUpgrade_Via_AccessManager` -- standard upgrade flow
- `Test_RevokeAdminRole` -- revoked admin cannot upgrade
- `Test_TransferUpgradeAuthority` -- two-step propose/accept authority transfer and migration verification
- `Test_AMtoAM_UpgradeAuthorityMigration` -- full AM-to-AM migration via propose + claim
