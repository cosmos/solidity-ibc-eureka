//! Attestation light client integration tests.
//!
//! Exercises the full IBC router lifecycle with attestation-based proof
//! verification instead of the mock light client. Attestor signatures are
//! produced off-chain and verified on-chain via ECDSA recovery.

use integration_tests::{
    admin::Admin,
    assert_commitment_set, assert_commitment_zeroed, assert_receipt_created, attestation,
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

mod attestor_limits;
mod send_recv_ack;
