# Solana IBC Integration Tests

Solana-to-Solana IBC integration tests using `ProgramTest` (BanksClient). Two independent chains run as separate `ProgramTest` instances with a mock light client that always accepts proofs, exercising the full IBC lifecycle without real proof verification.

## Architecture

```mermaid
graph LR
    subgraph "Test Harness"
        Test["Test Function"]
        Deployer["Deployer (Actor)"]
        Admin["Admin (Actor)"]
        IftAdmin["IftAdmin (Actor)"]
        User["User (Actor)"]
        Relayer["Relayer (Actor)"]
    end

    subgraph "Chain A (ProgramTest)"
        RA["ics26_router"]
        MCA["mock_light_client"]
        AMA["access_manager"]
        AppA["test_ibc_app / mock_ibc_app / ics27_gmp"]
    end

    subgraph "Chain B (ProgramTest)"
        RB["ics26_router"]
        MCB["mock_light_client"]
        AMB["access_manager"]
        AppB["test_ibc_app / mock_ibc_app / ics27_gmp"]
    end

    Test --> Admin
    Test --> IftAdmin
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

**Setup phase** — `ProgramTest` is configured with program binaries, `ProgramData` accounts (for upgrade authority verification) and pre-funded actors. No on-chain state exists yet.

**Runtime phase** — `start()` consumes the `ProgramTest`, produces a `BanksClient` and executes a sequence of real initialization transactions:

1. `access_manager::initialize` — creates the access manager account
2. `access_manager::grant_role` — grants `RELAYER_ROLE` and `ID_CUSTOMIZER_ROLE`
3. `ics26_router::initialize` — creates the router state
4. `mock_light_client::initialize` — creates client and consensus state accounts
5. `add_client` + `add_ibc_app` — registers the light client and IBC application
6. App-specific initialization (`test_ibc_app::initialize`, `ics27_gmp::initialize` + `test_gmp_app::initialize`, or nothing for `mock_ibc_app`)

After initialization, actors submit transactions and read account state.

## Program Variants

The `Program` enum lists programs to load onto a chain. IBC application variants register on a port and run initialization; auxiliary variants only load the binary.

| Variant        | Program loaded   | Port registration | Behavior                                                          |
| -------------- | ---------------- | ----------------- | ----------------------------------------------------------------- |
| `TestIbcApp`   | `test_ibc_app`   | Yes               | Stateful app that counts packets sent/received/acked/timed-out    |
| `MockIbcApp`   | `mock_ibc_app`   | Yes               | Stateless app with magic-string ack control (`RETURN_ERROR_ACK` etc.) |
| `Ics27Gmp`     | `ics27_gmp`      | Yes               | GMP IBC application on the GMP port                               |
| `TestGmpApp`   | `test_gmp_app`   | No                | Counter app invoked by GMP via CPI                                |
| `TestCpiProxy` | `test_cpi_proxy` | No                | Generic CPI proxy for security tests                              |
| `Ift`          | `ift`            | No                | Inter-chain fungible token transfers (uses GMP's port)            |
| `TestAccessManager` | `test_access_manager` | No           | Second AM instance for access manager migration tests             |

## Module Overview

| Module     | Purpose                                                                                              |
| ---------- | ---------------------------------------------------------------------------------------------------- |
| `chain`    | `Chain` struct with setup/runtime lifecycle, `ChainConfig`, `ChainAccounts`, `Program` enum, `add_counterparty` for multi-hop and `derive_mock_lc_pdas` |
| `accounts` | `anchor_discriminator` and `account_owned_by` helpers                                                |
| `actors`   | `Actor` trait and actor modules (`deployer`, `admin`, `ift_admin`, `user`, `relayer`)                 |
| `router`   | Instruction builders for `send_packet`, `recv_packet`, `ack_packet`, `timeout_packet`, chunk uploads, AM transfer (propose/accept/cancel) and `read_router_state` |
| `gmp`      | Instruction builders for GMP `send_call`, `recv_packet`, `ack_packet`, `timeout_packet`, raw `on_recv_packet` for security tests, AM transfer (propose/accept/cancel) and `read_gmp_app_state` |
| `ift`      | Instruction builders for IFT transfers, finalization, admin operations, pause, token creation (SPL and Token 2022), `TokenKind` enum and balance readers |

## Actors

```mermaid
graph TB
    Actor["trait Actor\npubkey()"]
    Deployer["Deployer\n- upgrade authority holder\n- program initialization"]
    Admin["Admin\n- ics26_propose/accept/cancel_am_transfer\n- gmp_propose/accept/cancel_am_transfer"]
    IftAdmin["IftAdmin\n- set_paused\n- propose/accept/cancel_admin\n- admin_mint"]
    User["User\n- send_packet\n- send_call\n- ift_transfer"]
    Relayer["Relayer\n- upload_chunks\n- cleanup_chunks\n- recv_packet\n- ack_packet\n- timeout_packet\n- gmp_recv_packet\n- gmp_ack_packet\n- gmp_timeout_packet\n- ift_gmp_ack_packet\n- ift_gmp_timeout_packet\n- ift_finalize_transfer"]

    Actor --> Deployer
    Actor --> Admin
    Actor --> IftAdmin
    Actor --> User
    Actor --> Relayer
```

All actors wrap a `Keypair`. `Deployer` holds the upgrade authority and initializes programs during `Chain::start()`; after initialization it transfers upgrade authority to the access manager PDA. `Admin` is an independent keypair whose pubkey is passed to the AM `initialize` instruction as the admin — it manages AM operations (role grants, AM transfers for ICS26 Router and GMP). `IftAdmin` manages IFT-specific admin operations (pause, admin transfer, minting) — a separate concern from the AM admin. `User` initiates IBC sends; `Relayer` bridges packets between chains and holds the `RELAYER_ROLE` in the access manager.

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

### IFT: transfer → ack → finalize

```mermaid
graph LR
    U["User"] -->|ift_transfer| A["Chain A\n(tokens burned,\ncommitment + PendingTransfer)"]
    A -->|"relayer observes commitment"| R["Relayer"]
    R -->|"upload_chunks + ift_gmp_ack_packet"| A2["Chain A\n(commitment zeroed\n+ GMPCallResult)"]
    R -->|"finalize_transfer"| A3["Chain A\n(PendingTransfer closed,\nsuccess: no-op / error: refund)"]
```

The IFT module supports both SPL Token and Token 2022 mints via the `TokenKind` enum. Tests use `setup_ift_chain` (SPL) or `setup_ift_chain_with_token` (either variant) to create a token, register an EVM bridge and mint an initial balance.

#### IFT Test Coverage

| Test | Scenario |
| --- | --- |
| `full_lifecycle` | Transfer → success ack → finalize (tokens stay burned) |
| `error_ack_refund` | Transfer → error ack → finalize (tokens refunded) |
| `timeout_refund` | Transfer → timeout → finalize (tokens refunded) |
| `batch_transfers` | Two consecutive transfers (seq 1 & 2), both acked and finalized |
| `token_2022_lifecycle` | Full lifecycle with Token 2022 mint (metadata extensions) |
| `admin_transfer` | Propose → accept admin; propose → cancel admin |
| `pause` | Pause blocks transfer + admin_mint; unpause restores them |

#### Admin Test Coverage

| Test | Scenario |
| --- | --- |
| `ics26_am_transfer_propose_accept` | Propose AM transfer on ICS26 Router, accept, verify `RouterState.am_state` updated |
| `ics26_am_transfer_propose_cancel` | Propose, cancel, verify pending cleared and AM unchanged |
| `ics26_am_transfer_unauthorized_propose` | Non-admin propose fails with `Unauthorized` |
| `gmp_am_transfer_propose_accept` | Propose AM transfer on GMP, accept, verify `GMPAppState.am_state` updated |
| `gmp_am_transfer_propose_cancel` | Propose, cancel on GMP |
| `gmp_am_transfer_unauthorized_propose` | Non-admin propose fails on GMP |

## Writing a New Test

A minimal router test that sends a packet from Chain A, delivers it to Chain B and acknowledges it back:

```rust
use super::*;

#[tokio::test]
async fn test_my_scenario() {
    // 1. Create actors
    let deployer = Deployer::new();
    let admin = Admin::new();
    let user = User::new();
    let relayer = Relayer::new();

    // 2. Configure chains with the programs they need
    let mut chain_a = Chain::new(ChainConfig {
        client_id: "a-client",
        counterparty_client_id: "b-client",
        deployer: &deployer,
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });
    chain_a.prefund(&user); // user needs SOL to submit transactions

    let mut chain_b = Chain::new(ChainConfig {
        client_id: "b-client",
        counterparty_client_id: "a-client",
        deployer: &deployer,
        admin: &admin,
        relayer: &relayer,
        programs: &[Program::TestIbcApp],
    });

    // 3. Start chains — runs all initialization transactions
    chain_a.start().await;
    chain_b.start().await;

    // 4. User sends a packet on Chain A
    let send = user
        .send_packet(
            &mut chain_a,
            SendPacketParams {
                sequence: 1,
                packet_data: b"hello",
                ..Default::default()
            },
        )
        .await
        .expect("send failed");

    assert_commitment_set(&chain_a, send.commitment_pda).await;

    // 5. Relayer uploads chunks and delivers to Chain B
    let (payload_pda, proof_pda) = relayer
        .upload_chunks(&mut chain_b, 1, b"hello", &[0u8; 32])
        .await
        .expect("upload failed");

    let recv = relayer
        .recv_packet(
            &mut chain_b,
            RecvPacketParams {
                sequence: 1,
                payload_chunk_pda: payload_pda,
                proof_chunk_pda: proof_pda,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("recv failed");

    assert_receipt_created(&chain_b, recv.receipt_pda).await;

    // 6. Relayer delivers ack back to Chain A
    let (ack_payload, ack_proof) = relayer
        .upload_chunks(&mut chain_a, 1, b"hello", &[0u8; 32])
        .await
        .expect("upload failed");

    let commitment_pda = relayer
        .ack_packet(
            &mut chain_a,
            AckPacketParams {
                sequence: 1,
                acknowledgement: br#"{"result": "AQ=="}"#.to_vec(),
                payload_chunk_pda: ack_payload,
                proof_chunk_pda: ack_proof,
                app_program: test_ibc_app::ID,
                ..Default::default()
            },
        )
        .await
        .expect("ack failed");

    assert_commitment_zeroed(&chain_a, commitment_pda).await;
}
```

Key patterns:

- **Setup before start** — `prefund`, program selection and `add_counterparty` (for multi-hop) must happen before `chain.start().await`.
- **Chunks before delivery** — every `recv_packet`, `ack_packet` and `timeout_packet` requires a preceding `upload_chunks` call.
- **`Default::default()`** — param structs implement `Default` for fields like `timeout_timestamp` and `proof_height`, so you only need to set what matters for your test.
- **Error assertions** — use `extract_custom_error` to match specific Anchor error codes instead of just checking that a transaction failed.

## Helper Functions

| Function | What it does | When to use |
| --- | --- | --- |
| `assert_commitment_set(chain, pda)` | Checks the commitment PDA has non-zero data | After `send_packet` to verify the commitment was stored |
| `assert_commitment_zeroed(chain, pda)` | Checks the commitment PDA was zeroed out | After `ack_packet` or `timeout_packet` to confirm consumption |
| `assert_receipt_created(chain, pda)` | Checks the receipt PDA exists and is owned by the router | After `recv_packet` to verify replay protection |
| `extract_ack_data(chain, pda)` | Reads the 32-byte ack commitment from a PDA | When you need to inspect the acknowledgement content |
| `extract_custom_error(err)` | Extracts the `u32` error code from a `BanksClientError` | When asserting a transaction failed with a specific Anchor error |
| `anchor_error_code(discriminant)` | Computes `6000 + discriminant` for an Anchor error variant | When constructing expected error codes from enum variants |

Pre-computed error constants are also available: `PACKET_COMMITMENT_MISMATCH`, `ASYNC_ACK_NOT_SUPPORTED`.

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
