//! MANUAL, LIVE validation harness (NOT run in normal CI).
//!
//! Goal: prove that the `MsgUpdateClient` produced by THIS crate's modified eth->cosmos
//! [`TxBuilder`] is ACCEPTED by the deployed, immutable `cw-ics08-wasm-eth-v1.3.0` light
//! client (default `08-wasm-262` on the `provider` chain) — i.e. that it does NOT trigger the
//! `"invalid client message: wasm contract call failed"` error seen with `proof-api:v0.8.0`.
//!
//! Two mechanisms are implemented:
//!
//! * Mechanism A (`gas_sim_update_client_is_accepted`, always compiled, `#[ignore]`):
//!   the most faithful reproduction of the production path. It drives the modified builder
//!   against the live RPCs, wraps the returned `TxBody` into a `TxRaw` with a DUMMY signature,
//!   and calls the provider's `cosmos.tx.v1beta1.Service/Simulate` (routed over the Tendermint
//!   RPC `abci_query` gateway). Gas simulation runs the full ante + message handlers — including
//!   the wasm client's `VerifyClientMessage` — WITHOUT verifying signatures and WITHOUT changing
//!   chain state. SUCCESS = a gas estimate with no `"invalid client message"` error.
//!
//! * Mechanism B (`local_verify_header_v1_3_0`, behind the `live-verify-v1_3_0` feature,
//!   `#[ignore]`): the deterministic offline backstop. It reads the on-chain (OLD-format)
//!   client/consensus state read-only, builds the OLD header(s) with the modified builder, and
//!   feeds them into the real `cw-ics08-wasm-eth-v1.3.0` `ethereum-light-client::verify_header`
//!   (pulled at its release tag) with that crate's `TestBlsVerifier`. `verify_header().is_ok()`
//!   for every header == the v1.3.0 verification logic the wasm wraps accepts the output.
//!
//! SAFETY: read-only / simulate-only. No state-changing tx is ever broadcast and no real signing
//! key is used (a 64-byte zero "signature" is enough for Simulate). All endpoints/creds are read
//! from ENV VARS — nothing is hardcoded. The tests are `#[ignore]` so normal `cargo test` skips
//! them; run them explicitly (see below).
//!
//! ## How to run
//!
//! ```bash
//! export ETH_RPC_URL='https://eth-sepolia.g.alchemy.com/v2/<key>'
//! export ETH_BEACON_API_URL='https://user:pass@<lodestar-sepolia-host>'   # basic-auth in URL
//! export TENDERMINT_RPC_URL='https://user:pass@<provider-tendermint-rpc-host>'
//! # Optional overrides (defaults shown):
//! export DST_CLIENT_ID='08-wasm-262'
//! export ICS26_ADDRESS='0x3fcBB8b5d85FB5F77603e11536b5E90FeE37e6c0'
//! export SIGNER_ADDRESS='cosmos127tlxptdqt0pe25mq5cf8ju68lsa9yxy885vqm'
//! # export SIGNER_SEQUENCE='2178'   # optional; auto-queried from auth if unset
//!
//! # Mechanism A (gas-sim against the live wasm client):
//! cargo test -p proof-api-eth-to-cosmos --test live_v1_3_0_acceptance \
//!     -- --ignored --nocapture gas_sim_update_client_is_accepted
//!
//! # Mechanism B (offline verify_header against the v1.3.0 crate):
//! cargo test -p proof-api-eth-to-cosmos --features live-verify-v1_3_0 \
//!     --test live_v1_3_0_acceptance -- --ignored --nocapture local_verify_header_v1_3_0
//! ```
//!
//! NOTE: the builder's `update_client` blocks on `wait_for_cosmos_chain_to_catch_up` (up to
//! ~15 min) and needs live beacon + Tendermint RPC during the run. Only the literal
//! `"invalid client message"` / `"wasm contract call failed"` error is the decisive rejection;
//! RPC/beacon/account-sequence errors are infrastructure noise and are reported as such.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::missing_panics_doc)]

use std::str::FromStr;

use alloy::{
    primitives::Address,
    providers::{Provider, RootProvider},
};
use ibc_eureka_utils::rpc::TendermintRpcExt;
use proof_api_eth_to_cosmos::tx_builder::TxBuilder;
use proof_api_lib::tx_builder::TxBuilderService;
use prost::Message;
use tendermint_rpc::{Client, HttpClient};

use ibc_proto_eureka::cosmos::{
    auth::v1beta1::{BaseAccount, QueryAccountRequest, QueryAccountResponse},
    tx::v1beta1::{
        mode_info::{Single, Sum},
        AuthInfo, Fee, ModeInfo, SignerInfo, SimulateRequest, SimulateResponse, TxRaw,
    },
};

const DEFAULT_ICS26_ADDRESS: &str = "0x3fcBB8b5d85FB5F77603e11536b5E90FeE37e6c0";
const DEFAULT_DST_CLIENT_ID: &str = "08-wasm-262";
const DEFAULT_SIGNER_ADDRESS: &str = "cosmos127tlxptdqt0pe25mq5cf8ju68lsa9yxy885vqm";

/// `SignMode::Direct` discriminant (`cosmos.tx.signing.v1beta1.SignMode`).
const SIGN_MODE_DIRECT: i32 = 1;

/// Reads a required env var or panics with an actionable message.
fn require_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "missing required env var `{name}`; this is a manual harness, see the module docs for \
             the full list (ETH_RPC_URL, ETH_BEACON_API_URL, TENDERMINT_RPC_URL)"
        )
    })
}

fn ics26_address() -> Address {
    let raw = std::env::var("ICS26_ADDRESS").unwrap_or_else(|_| DEFAULT_ICS26_ADDRESS.to_string());
    Address::from_str(&raw).expect("ICS26_ADDRESS is not a valid EVM address")
}

fn dst_client_id() -> String {
    std::env::var("DST_CLIENT_ID").unwrap_or_else(|_| DEFAULT_DST_CLIENT_ID.to_string())
}

fn signer_address() -> String {
    std::env::var("SIGNER_ADDRESS").unwrap_or_else(|_| DEFAULT_SIGNER_ADDRESS.to_string())
}

/// Builds the modified eth->cosmos [`TxBuilder`] against the live endpoints from env.
async fn build_tx_builder() -> TxBuilder<RootProvider> {
    let eth_rpc = require_env("ETH_RPC_URL");
    let beacon_url = require_env("ETH_BEACON_API_URL");
    let tm_rpc = require_env("TENDERMINT_RPC_URL");

    let provider: RootProvider = RootProvider::builder()
        .connect(&eth_rpc)
        .await
        .expect("connect to Sepolia ETH RPC (ETH_RPC_URL)");

    let tm_client = HttpClient::from_rpc_url(&tm_rpc);

    TxBuilder::new(
        ics26_address(),
        provider,
        beacon_url,
        tm_client,
        signer_address(),
    )
}

/// Queries the signer's on-chain `account_number`/`sequence` via the auth gRPC-over-ABCI gateway.
/// Returns `(account_number, sequence)`. `SIGNER_SEQUENCE` env overrides the queried sequence.
async fn query_account(tm: &HttpClient, address: &str) -> (u64, u64) {
    let req = QueryAccountRequest {
        address: address.to_string(),
    };
    let res = tm
        .abci_query(
            Some("/cosmos.auth.v1beta1.Query/Account".to_string()),
            req.encode_to_vec(),
            None,
            false,
        )
        .await
        .expect("auth Account abci_query transport");

    assert!(
        !res.code.is_err(),
        "auth Account query returned non-zero code {}: {}",
        res.code.value(),
        res.log
    );

    let account_any = QueryAccountResponse::decode(res.value.as_slice())
        .expect("decode QueryAccountResponse")
        .account
        .expect("account not found on chain (does SIGNER_ADDRESS exist on `provider`?)");
    let base = BaseAccount::decode(account_any.value.as_slice()).expect("decode BaseAccount");

    let sequence = std::env::var("SIGNER_SEQUENCE")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(base.sequence);

    (base.account_number, sequence)
}

/// Mechanism A: drive the modified builder, then gas-simulate the resulting `MsgUpdateClient`
/// against the live, deployed v1.3.0 wasm client. Decisive signal: no `"invalid client message"`.
#[tokio::test]
#[ignore = "manual, live: requires ETH_RPC_URL, ETH_BEACON_API_URL, TENDERMINT_RPC_URL"]
async fn gas_sim_update_client_is_accepted() {
    let client_id = dst_client_id();
    let signer = signer_address();

    let tx_builder = build_tx_builder().await;
    let tm = HttpClient::from_rpc_url(&require_env("TENDERMINT_RPC_URL"));

    // 1) Build the MsgUpdateClient TxBody with the MODIFIED (OLD v1.3.0 schema) builder.
    //    This is the exact thing proof-api:v0.8.0 fails on at gas simulation.
    eprintln!("[A] building update_client tx for {client_id} (this can block up to ~15 min)...");
    let body_bytes = tx_builder
        .update_client(client_id.clone())
        .await
        .expect("update_client should produce a MsgUpdateClient TxBody (infra error if it panics)");
    eprintln!(
        "[A] update_client returned {} TxBody bytes",
        body_bytes.len()
    );

    // 2) Assemble a TxRaw with a DUMMY signature (Simulate skips signature verification).
    let (account_number, sequence) = query_account(&tm, &signer).await;
    eprintln!("[A] signer account_number={account_number} sequence={sequence}");

    let auth_info = AuthInfo {
        signer_infos: vec![SignerInfo {
            // public_key is optional for existing accounts; the verifier looks it up by address.
            public_key: None,
            mode_info: Some(ModeInfo {
                sum: Some(Sum::Single(Single {
                    mode: SIGN_MODE_DIRECT,
                })),
            }),
            sequence,
        }],
        fee: Some(Fee {
            amount: vec![],
            gas_limit: 0,
            payer: String::new(),
            granter: String::new(),
        }),
        ..Default::default()
    };

    let tx_raw = TxRaw {
        body_bytes,
        auth_info_bytes: auth_info.encode_to_vec(),
        // DUMMY 64-byte signature: Simulate does NOT verify it and does NOT change state.
        signatures: vec![vec![0u8; 64]],
    };

    // 3) Call Simulate over the Tendermint RPC abci_query gateway (direct gRPC is flaky).
    let sim_req = SimulateRequest {
        tx_bytes: tx_raw.encode_to_vec(),
        ..Default::default()
    };
    let res = tm
        .abci_query(
            Some("/cosmos.tx.v1beta1.Service/Simulate".to_string()),
            sim_req.encode_to_vec(),
            None,
            false,
        )
        .await
        .expect("Simulate abci_query transport");

    let log_lower = res.log.to_lowercase();
    let decisive_rejection = log_lower.contains("invalid client message")
        || log_lower.contains("wasm contract call failed");

    if res.code.is_err() {
        // Classify: decisive wasm rejection vs. infra noise.
        assert!(
            !decisive_rejection,
            "DECISIVE FAILURE: the modified builder's MsgUpdateClient was REJECTED by the live \
             v1.3.0 wasm client `{client_id}`. Simulate code {}, log:\n{}",
            res.code.value(),
            res.log
        );
        panic!(
            "Simulate returned a NON-decisive (infra) error for `{client_id}` (NOT the wasm \
             VerifyClientMessage signal). Re-check endpoints / re-query SIGNER_SEQUENCE and retry. \
             code {}, log:\n{}",
            res.code.value(),
            res.log
        );
    }

    // code == 0: the tx (incl. wasm VerifyClientMessage) executed in simulation. Fix validated.
    let sim_resp = SimulateResponse::decode(res.value.as_slice()).expect("decode SimulateResponse");
    let gas_used = sim_resp.gas_info.as_ref().map_or(0, |g| g.gas_used);
    let gas_wanted = sim_resp.gas_info.as_ref().map_or(0, |g| g.gas_wanted);
    eprintln!(
        "[A] SUCCESS: live v1.3.0 client `{client_id}` ACCEPTED the modified builder's \
         MsgUpdateClient. gas_used={gas_used} gas_wanted={gas_wanted}"
    );
    assert!(
        gas_used > 0,
        "expected a non-zero gas estimate on success, got SimulateResponse: {sim_resp:?}"
    );
}

/// Mechanism B: offline verification against the real `cw-ics08-wasm-eth-v1.3.0` crate.
#[cfg(feature = "live-verify-v1_3_0")]
mod local_verify {
    use super::{
        build_tx_builder, dst_client_id, require_env, HttpClient, Message, TendermintRpcExt,
        TxBuilderService,
    };

    use alloy::primitives::{FixedBytes, B256};
    use ethereum_light_client_v1_3_0::{
        client_state::ClientState,
        consensus_state::ConsensusState,
        header::Header,
        test_utils::bls_verifier::{aggreagate, fast_aggregate_verify},
        update::update_consensus_state,
        verify::{verify_header, BlsVerify},
    };
    use ibc_proto_eureka::ibc::{
        core::client::v1::MsgUpdateClient,
        lightclients::wasm::v1::{
            ClientMessage, ClientState as WasmClientState, ConsensusState as WasmConsensusState,
        },
    };

    /// A `BlsVerify` impl delegating to the v1.3.0 crate's `test-utils` BLS implementation.
    /// The signature/pubkey types are `alloy_primitives::FixedBytes<N>` aliases shared with the
    /// v1.3.0 crate, so this matches the trait definition exactly.
    struct TestBls;

    impl BlsVerify for TestBls {
        type Error = ethereum_light_client_v1_3_0::test_utils::bls_verifier::BlsError;

        fn fast_aggregate_verify(
            &self,
            public_keys: &[FixedBytes<48>],
            msg: B256,
            signature: FixedBytes<96>,
        ) -> Result<(), Self::Error> {
            fast_aggregate_verify(public_keys, msg, signature)
        }

        fn aggregate(&self, public_keys: &[FixedBytes<48>]) -> Result<FixedBytes<48>, Self::Error> {
            aggreagate(public_keys)
        }
    }

    /// Decodes the OLD `Header` JSONs out of an `update_client` `TxBody`'s `MsgUpdateClient`s.
    fn headers_from_tx_body(body_bytes: &[u8]) -> Vec<Header> {
        use ibc_proto_eureka::cosmos::tx::v1beta1::TxBody;
        let body = TxBody::decode(body_bytes).expect("decode TxBody");
        body.messages
            .iter()
            .filter(|m| m.type_url == "/ibc.core.client.v1.MsgUpdateClient")
            .map(|m| {
                let msg =
                    MsgUpdateClient::decode(m.value.as_slice()).expect("decode MsgUpdateClient");
                let client_msg = ClientMessage::decode(
                    msg.client_message.expect("client_message").value.as_slice(),
                )
                .expect("decode wasm ClientMessage");
                serde_json::from_slice::<Header>(&client_msg.data)
                    .expect("OLD Header JSON must parse into the v1.3.0 crate's Header type")
            })
            .collect()
    }

    #[tokio::test]
    #[ignore = "manual, live: requires ETH_RPC_URL, ETH_BEACON_API_URL, TENDERMINT_RPC_URL"]
    async fn local_verify_header_v1_3_0() {
        let client_id = dst_client_id();
        let tm = HttpClient::from_rpc_url(&require_env("TENDERMINT_RPC_URL"));

        // 1) Read the on-chain (OLD v1.3.0 schema) client + consensus state, read-only.
        let cs_any = tm
            .client_state(client_id.clone())
            .await
            .expect("query client state");
        let wasm_cs =
            WasmClientState::decode(cs_any.value.as_slice()).expect("decode WasmClientState");
        let mut client_state: ClientState =
            serde_json::from_slice(&wasm_cs.data).expect("decode v1.3.0 ClientState JSON");

        let cons_any = tm
            .consensus_state(client_id.clone(), 0)
            .await
            .expect("query consensus state");
        let wasm_cons = WasmConsensusState::decode(cons_any.value.as_slice())
            .expect("decode WasmConsensusState");
        let mut consensus_state: ConsensusState =
            serde_json::from_slice(&wasm_cons.data).expect("decode v1.3.0 ConsensusState JSON");

        eprintln!(
            "[B] on-chain trusted slot={} exec_block={}",
            client_state.latest_slot, client_state.latest_execution_block_number
        );

        // 2) Build the OLD header(s) with the modified builder.
        let tx_builder = build_tx_builder().await;
        eprintln!("[B] building update_client tx (can block up to ~15 min)...");
        let body_bytes = tx_builder
            .update_client(client_id.clone())
            .await
            .expect("update_client TxBody");
        let headers = headers_from_tx_body(&body_bytes);
        assert!(!headers.is_empty(), "expected at least one OLD Header");
        eprintln!("[B] produced {} OLD header(s)", headers.len());

        // 3) Verify each header with the real v1.3.0 verify_header, advancing state per header
        //    exactly like the wasm contract does.
        for (i, header) in headers.into_iter().enumerate() {
            // current_timestamp must be >= the update's signature slot timestamp.
            let current_timestamp =
                header.consensus_update.attested_header.execution.timestamp + 1000;

            verify_header(&consensus_state, &client_state, current_timestamp, &header, TestBls)
                .unwrap_or_else(|e| {
                    panic!(
                        "DECISIVE FAILURE: v1.3.0 verify_header REJECTED header[{i}] for `{client_id}`: {e}"
                    )
                });
            eprintln!("[B] header[{i}] ACCEPTED by v1.3.0 verify_header");

            let (_, new_cons, new_client) =
                update_consensus_state(consensus_state.clone(), client_state.clone(), header)
                    .expect("update_consensus_state");
            consensus_state = new_cons;
            if let Some(new_client) = new_client {
                client_state = new_client;
            }
        }

        eprintln!(
            "[B] SUCCESS: ALL headers accepted by cw-ics08-wasm-eth-v1.3.0 verify_header for `{client_id}`"
        );
    }
}
