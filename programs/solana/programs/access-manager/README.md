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
graph TD
    subgraph setup["① Setup (one-time)"]
        direction LR
        Deployer["Deployer"]:::deployer
        PD_S["ProgramData\nauthority: deployer"]:::bpf
        AM_S["AccessManager PDA"]:::am

        Deployer -->|"1. deploy program"| PD_S
        Deployer -->|"2. initialize AM\ngrant ADMIN_ROLE"| AM_S
        Deployer -->|"3. set-upgrade-authority\ndeployer → UA PDA"| PD_S
    end

    subgraph upgrade["② Upgrade"]
        direction LR
        Deployer2["Deployer"]:::deployer
        Admin["Admin\n(ADMIN_ROLE)"]:::admin
        Buffer["Buffer Account\nnew bytecode"]:::bpf
        UPIX["upgrade_program()"]:::ix
        AM_U["AccessManager PDA"]:::am
        UA_U["Upgrade Authority PDA"]:::pda
        PD_U["ProgramData\nauthority: UA PDA"]:::bpf
        Target["Target Program"]:::target

        Deployer2 -->|"1. write-buffer +\nset-buffer-authority → UA PDA"| Buffer
        Admin -->|"2. call upgrade_program()"| UPIX
        UPIX -->|"3. require_admin check"| AM_U
        UPIX -->|"4. invoke_signed\n(PDA signs)"| UA_U
        UA_U -->|"5. BPFLoader::upgrade()"| PD_U
        Buffer -->|"bytecode source"| PD_U
        PD_U -->|"6. bytecode replaced"| Target
    end

    setup ~~~ upgrade

    style setup fill:#FFF3E0,stroke:#E65100,color:#000
    style upgrade fill:#E8F5E9,stroke:#2E7D32,color:#000

    classDef admin fill:#4CAF50,stroke:#2E7D32,color:#fff
    classDef deployer fill:#FF9800,stroke:#E65100,color:#fff
    classDef am fill:#2196F3,stroke:#1565C0,color:#fff
    classDef pda fill:#9C27B0,stroke:#6A1B9A,color:#fff
    classDef ix fill:#00BCD4,stroke:#00838F,color:#fff
    classDef bpf fill:#607D8B,stroke:#37474F,color:#fff
    classDef target fill:#795548,stroke:#4E342E,color:#fff
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
graph LR
    subgraph Actors
        Admin["Admin\n(ADMIN_ROLE)"]:::admin
        NewAuth["New Authority\n(keypair or PDA)"]:::newauth
    end

    subgraph AM["Access Manager"]
        AM_State["AccessManager PDA\npending_authority_transfer"]:::am
        UA_PDA["Upgrade Authority PDA\ncurrent authority"]:::pda
        PROPOSE["propose_upgrade_authority_transfer()"]:::ix
        ACCEPT["accept_upgrade_authority_transfer()"]:::ix
    end

    subgraph BPFLoader["BPF Loader Upgradeable"]
        PD["ProgramData\nupgrade_authority"]:::bpf
    end

    subgraph Target["Target Program"]
        Program["Program Account"]:::target
    end

    Admin -->|"1. propose transfer\n(target, new_authority)"| PROPOSE
    PROPOSE -->|"2. require_admin check"| AM_State
    PROPOSE -->|"3. set pending"| AM_State
    NewAuth -->|"4. accept transfer\n(signer = new_authority)"| ACCEPT
    ACCEPT -->|"5. invoke_signed\n(PDA signs)"| UA_PDA
    UA_PDA -->|"6. BPFLoader::set_authority()\nold -> new"| PD
    PD -->|"authority now: New Authority"| NewAuth
    NewAuth -->|"7. can now upgrade\ndirectly or via new AM"| Program

    classDef admin fill:#4CAF50,stroke:#2E7D32,color:#fff
    classDef newauth fill:#E91E63,stroke:#AD1457,color:#fff
    classDef am fill:#2196F3,stroke:#1565C0,color:#fff
    classDef pda fill:#9C27B0,stroke:#6A1B9A,color:#fff
    classDef ix fill:#00BCD4,stroke:#00838F,color:#fff
    classDef bpf fill:#607D8B,stroke:#37474F,color:#fff
    classDef target fill:#795548,stroke:#4E342E,color:#fff
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
sequenceDiagram
    participant Admin as AM-A Admin
    participant AMA as AM-A
    participant AMB as AM-B
    participant BPF as BPF Loader
    participant Anyone as Anyone (permissionless)

    Admin->>AMA: propose_upgrade_authority_transfer(target, AM-B's PDA)
    AMA->>AMA: store pending transfer

    Note over Anyone,AMB: No admin role needed -- PDA signing is the authorization
    Anyone->>AMB: claim_upgrade_authority(target)
    AMB->>AMA: CPI: accept_upgrade_authority_transfer(target)
    AMA->>AMA: validate pending matches AM-B's PDA
    AMA->>BPF: invoke_signed: SetAuthority(old=AM-A PDA, new=AM-B PDA)
    BPF->>BPF: ProgramData.authority = AM-B's PDA
    AMA->>AMA: clear pending transfer
```

Repeat for each managed program (ICS07, ICS26, GMP, etc.).

#### Runtime role migration

Each IBC program (ICS07, ICS26, GMP, attestation) stores a `access_manager: Pubkey` field in its state that points to the access manager it delegates role checks to. Calling `set_access_manager` on each program repoints it from AM-A to AM-B:

```mermaid
sequenceDiagram
    participant Admin as AM-B Admin
    participant ICS26 as ICS26 Router
    participant AMB as AM-B

    Note over Admin,AMB: Admin must have ADMIN_ROLE on the current AM<br/>(AM-A if not yet migrated, AM-B if already migrated)
    Admin->>ICS26: set_access_manager(AM-B program ID)
    ICS26->>ICS26: RouterState.access_manager = AM-B
    Note over ICS26,AMB: Future role checks (relay, pause, etc.)<br/>now read from AM-B's state
```

Repeat for each IBC program. IFT uses a different pattern (`admin: Pubkey` with two-step propose/accept transfer) and does not use `set_access_manager`.

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
