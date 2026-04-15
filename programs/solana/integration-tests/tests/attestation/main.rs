//! Attestation light client integration tests.
//!
//! Exercises the full IBC router lifecycle with attestation-based proof
//! verification instead of the mock light client. Attestor signatures are
//! produced off-chain and verified on-chain via ECDSA recovery.

use anchor_lang::AccountDeserialize;
use integration_tests::{
    admin::Admin,
    assert_commitment_set, assert_commitment_zeroed, assert_receipt_created,
    attestation as att_helpers,
    attestor::Attestors,
    chain::{Chain, ChainProgram},
    deployer::Deployer,
    extract_ack_data,
    programs::{AttestationLc, TestIbcApp},
    relayer::Relayer,
    router::{self, AckPacketParams, RecvPacketParams, SendPacketParams, PROOF_HEIGHT},
    user::User,
};
use solana_ibc_types::ics24;

/// Attestation program ID re-exported for chain construction.
const ATTESTATION_PROGRAM_ID: solana_sdk::pubkey::Pubkey = attestation::ID;

mod send_recv_ack;
