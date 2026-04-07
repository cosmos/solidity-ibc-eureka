# Solana IBC Integration Tests

Solana-to-Solana IBC integration tests using `ProgramTest` (BanksClient). Two independent chains run as separate `ProgramTest` instances with a mock light client that always accepts proofs, exercising the full IBC lifecycle without real proof verification.

## Architecture

```mermaid
graph LR
    subgraph "Test Harness"
        Test["Test Function"]
        User["User (Actor)"]
        Relayer["Relayer (Actor)"]
    end

    subgraph "Chain A (ProgramTest)"
        RA["ics26_router"]
        MCA["mock_light_client"]
        AMA["access_manager"]
        AppA["test_ibc_app / ics27_gmp"]
    end

    subgraph "Chain B (ProgramTest)"
        RB["ics26_router"]
        MCB["mock_light_client"]
        AMB["access_manager"]
        AppB["test_ibc_app / ics27_gmp"]
    end

    Test --> User
    Test --> Relayer
    User -->|send_packet / send_call| RA
    Relayer -->|upload_chunks + recv_packet| RB
    Relayer -->|upload_chunks + ack_packet| RA
    Relayer -->|upload_chunks + timeout_packet| RA
    RA -->|CPI| MCA
    RA -->|CPI| AMA
    RA -->|CPI| AppA
    RB -->|CPI| MCB
    RB -->|CPI| AMB
    RB -->|CPI| AppB
```

## Two-Phase Chain Lifecycle

Each `Chain` follows a setup-then-runtime lifecycle:

```mermaid
flowchart LR
    subgraph Setup["Setup Phase (before start)"]
        direction TB
        New["Chain::new(config)"] --> Prefund["prefund(actor)"]
        Prefund --> Start["chain.start().await"]
    end

    subgraph Runtime["Runtime Phase (after start)"]
        direction TB
        Send["User: send_packet / send_call"] --> Upload["Relayer: upload_chunks"]
        Upload --> Deliver["Relayer: recv_packet / ack_packet / timeout_packet"]
        Deliver --> Verify["get_account + assertions"]
    end

    Setup --> Runtime
```

**Setup phase** — `ProgramTest` is configured with programs, pre-funded accounts and on-chain state (router, client, access manager). Nothing is running yet.

**Runtime phase** — `start()` consumes the `ProgramTest` and produces a `BanksClient`. Actors submit transactions and read account state.

## Module Overview

| Module     | Purpose                                                                                              |
| ---------- | ---------------------------------------------------------------------------------------------------- |
| `chain`    | `Chain` struct with setup/runtime lifecycle, `ChainConfig`, `ChainAccounts`                          |
| `accounts` | Anchor serialization helpers, state setup (router, client, access manager)                           |
| `router`   | Instruction builders for `send_packet`, `recv_packet`, `ack_packet`, `timeout_packet`, chunk uploads |
| `gmp`      | Instruction builders for GMP `send_call`, `recv_packet`, `ack_packet`, `timeout_packet`              |
| `user`     | `User` actor — sends packets and GMP calls                                                           |
| `relayer`  | `Relayer` actor — uploads chunks and delivers recv/ack/timeout packets                               |

## Actors

```mermaid
graph TB
    Actor["trait Actor\npubkey()"]
    User["User\n- send_packet\n- send_call"]
    Relayer["Relayer\n- upload_chunks\n- recv_packet\n- ack_packet\n- timeout_packet\n- gmp_recv_packet\n- gmp_ack_packet\n- gmp_timeout_packet"]

    Actor --> User
    Actor --> Relayer
```

Both actors wrap a `Keypair`. The `User` initiates IBC sends; the `Relayer` bridges packets between chains and holds the `RELAYER_ROLE` in the access manager.

## Packet Flow

Before each packet delivery, the relayer uploads payload and proof data to on-chain chunk PDAs via `upload_payload_chunk`/`upload_proof_chunk` transactions. The router reads those chunks during instruction execution.

### Router: send → recv → ack

```mermaid
graph LR
    U["User"] -->|send_packet| A["Chain A\n(commitment created)"]
    A -->|"relayer observes commitment"| R["Relayer"]
    R -->|"upload_chunks + recv_packet"| B["Chain B\n(receipt + ack created)"]
    B -->|"relayer observes ack"| R
    R -->|"upload_chunks + ack_packet"| A2["Chain A\n(commitment zeroed)"]
```

### Router: send → timeout

```mermaid
graph LR
    U["User"] -->|send_packet| A["Chain A\n(commitment created)"]
    A -->|"packet expires"| R["Relayer"]
    R -->|"upload_chunks + timeout_packet"| A2["Chain A\n(commitment zeroed)"]
```

### GMP: send_call → recv → ack

```mermaid
graph LR
    U["User"] -->|send_call| A["Chain A\n(commitment created)"]
    A -->|"relayer observes commitment"| R["Relayer"]
    R -->|"upload_chunks + gmp_recv_packet"| B["Chain B\n(receipt + ack + app CPI)"]
    B -->|"relayer observes ack"| R
    R -->|"upload_chunks + gmp_ack_packet"| A2["Chain A\n(commitment zeroed\n+ GMPCallResult)"]
```

### GMP: send_call → timeout

```mermaid
graph LR
    U["User"] -->|send_call| A["Chain A\n(commitment created)"]
    A -->|"packet expires"| R["Relayer"]
    R -->|"upload_chunks + gmp_timeout_packet"| A2["Chain A\n(commitment zeroed\n+ GMPCallResult timeout)"]
```

## Test Matrix

| Test                               | File                  | Flow                                               |
| ---------------------------------- | --------------------- | -------------------------------------------------- |
| `test_full_packet_lifecycle`       | `router_lifecycle.rs` | send → recv → ack                                  |
| `test_bidirectional_packets`       | `router_lifecycle.rs` | A→B and B→A with different sequences               |
| `test_multiple_sequential_packets` | `router_lifecycle.rs` | 3 packets: send all → recv all → ack all           |
| `test_timeout_packet`              | `router_lifecycle.rs` | send → timeout                                     |
| `test_gmp_full_lifecycle`          | `gmp_lifecycle.rs`    | GMP send_call → recv (CPI into test_gmp_app) → ack |
| `test_gmp_timeout`                 | `gmp_lifecycle.rs`    | GMP send_call → timeout                            |

## Running

```bash
# Build all required .so binaries first
just build-solana

# Run all integration tests
cargo test -p integration-tests

# Run a specific test with logs
cargo test -p integration-tests test_full_packet_lifecycle -- --nocapture
```

Programs are loaded from `target/deploy/` via `SBF_OUT_DIR`. After modifying any program source, rebuild with `just build-solana <program>` before re-running tests.
