use crate::accounts::anchor_discriminator;
use crate::chain::ChainAccounts;
use crate::router::RecvResult;
use anchor_lang::InstructionData;
use ics26_router::state::*;
use prost::Message as ProstMessage;
use solana_ibc_proto::{RawGmpPacketData, RawGmpSolanaPayload, RawSolanaAccountMeta};
use solana_ibc_types::Payload;
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

pub const GMP_PORT_ID: &str = "gmpport";
pub const ICS27_VERSION: &str = "ics27-2";
pub const ICS27_ENCODING_PROTOBUF: &str = "application/x-protobuf";

// ── Send Call ───────────────────────────────────────────────────────────

pub struct GmpSendCallParams<'a> {
    pub sequence: u64,
    pub timeout_timestamp: u64,
    pub receiver: &'a str,
    pub payload: Vec<u8>,
}

pub fn build_gmp_send_call_ix(
    sender: Pubkey,
    payer: Pubkey,
    accounts: &ChainAccounts,
    client_id: &str,
    params: GmpSendCallParams<'_>,
) -> (Instruction, Pubkey) {
    let (gmp_app_state_pda, _) =
        Pubkey::find_program_address(&[ics27_gmp::state::GMPAppState::SEED], &ics27_gmp::ID);
    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (commitment_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            client_id.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );
    let (ibc_app_pda, _) =
        Pubkey::find_program_address(&[IBCApp::SEED, GMP_PORT_ID.as_bytes()], &ics26_router::ID);
    let (client_pda, _) = Pubkey::find_program_address(
        &[Client::SEED, client_id.as_bytes()],
        &ics26_router::ID,
    );

    let msg = ics27_gmp::state::SendCallMsg {
        source_client: client_id.to_string(),
        sequence: params.sequence,
        timeout_timestamp: params.timeout_timestamp,
        receiver: params.receiver.to_string(),
        salt: vec![],
        payload: params.payload,
        memo: String::new(),
        encoding: ICS27_ENCODING_PROTOBUF.to_string(),
    };

    let ix = Instruction {
        program_id: ics27_gmp::ID,
        accounts: vec![
            AccountMeta::new(gmp_app_state_pda, false),
            AccountMeta::new_readonly(sender, true),
            AccountMeta::new(payer, true),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new(commitment_pda, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new_readonly(client_pda, false),
            AccountMeta::new_readonly(mock_light_client::ID, false),
            AccountMeta::new_readonly(accounts.mock_client_state, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new_readonly(accounts.mock_consensus_state, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data: ics27_gmp::instruction::SendCall { msg }.data(),
    };

    (ix, commitment_pda)
}

// ── Recv Packet ─────────────────────────────────────────────────────────

pub struct GmpRecvPacketParams {
    pub sequence: u64,
    pub payload_chunk_pda: Pubkey,
    pub proof_chunk_pda: Pubkey,
    pub remaining_accounts: Vec<AccountMeta>,
}

pub fn build_gmp_recv_packet_ix(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    dest_client: &str,
    source_client: &str,
    clock_time: i64,
    params: GmpRecvPacketParams,
) -> RecvResult {
    let gmp_app_state_pda = accounts
        .gmp_app_state_pda
        .expect("GMP chain required for gmp_recv_packet");

    let gmp_accounts = ChainAccounts {
        app_state_pda: gmp_app_state_pda,
        ..*accounts
    };

    crate::router::build_recv_packet_ix(
        relayer,
        &gmp_accounts,
        dest_client,
        source_client,
        clock_time,
        crate::router::RecvPacketParams {
            sequence: params.sequence,
            payload_chunk_pda: params.payload_chunk_pda,
            proof_chunk_pda: params.proof_chunk_pda,
            port_id: GMP_PORT_ID,
            version: ICS27_VERSION,
            encoding: ICS27_ENCODING_PROTOBUF,
            app_program: ics27_gmp::ID,
            extra_remaining_accounts: params.remaining_accounts,
        },
    )
}

// ── Ack Packet ──────────────────────────────────────────────────────────

pub struct GmpAckPacketParams {
    pub sequence: u64,
    pub acknowledgement: Vec<u8>,
    pub payload_chunk_pda: Pubkey,
    pub proof_chunk_pda: Pubkey,
}

pub fn build_gmp_ack_packet_ix(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    source_client: &str,
    dest_client: &str,
    clock_time: i64,
    params: GmpAckPacketParams,
) -> (Instruction, Pubkey) {
    let gmp_app_state_pda = accounts
        .gmp_app_state_pda
        .expect("GMP chain required for gmp_ack_packet");

    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(source_client, params.sequence, &ics27_gmp::ID);

    let gmp_accounts = ChainAccounts {
        app_state_pda: gmp_app_state_pda,
        ..*accounts
    };

    crate::router::build_ack_packet_ix(
        relayer,
        &gmp_accounts,
        source_client,
        dest_client,
        clock_time,
        crate::router::AckPacketParams {
            sequence: params.sequence,
            acknowledgement: params.acknowledgement,
            payload_chunk_pda: params.payload_chunk_pda,
            proof_chunk_pda: params.proof_chunk_pda,
            port_id: GMP_PORT_ID,
            version: ICS27_VERSION,
            encoding: ICS27_ENCODING_PROTOBUF,
            app_program: ics27_gmp::ID,
            extra_remaining_accounts: vec![AccountMeta::new(result_pda, false)],
        },
    )
}

// ── Timeout Packet ─────────────────────────────────────────────────────

pub struct GmpTimeoutPacketParams {
    pub sequence: u64,
    pub payload_chunk_pda: Pubkey,
    pub proof_chunk_pda: Pubkey,
}

pub fn build_gmp_timeout_packet_ix(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    source_client: &str,
    dest_client: &str,
    clock_time: i64,
    params: GmpTimeoutPacketParams,
) -> (Instruction, Pubkey) {
    let gmp_app_state_pda = accounts
        .gmp_app_state_pda
        .expect("GMP chain required for gmp_timeout_packet");

    let (result_pda, _) =
        solana_ibc_types::GMPCallResult::pda(source_client, params.sequence, &ics27_gmp::ID);

    let gmp_accounts = ChainAccounts {
        app_state_pda: gmp_app_state_pda,
        ..*accounts
    };

    crate::router::build_timeout_packet_ix(
        relayer,
        &gmp_accounts,
        source_client,
        dest_client,
        clock_time,
        crate::router::TimeoutPacketParams {
            sequence: params.sequence,
            payload_chunk_pda: params.payload_chunk_pda,
            proof_chunk_pda: params.proof_chunk_pda,
            port_id: GMP_PORT_ID,
            version: ICS27_VERSION,
            encoding: ICS27_ENCODING_PROTOBUF,
            app_program: ics27_gmp::ID,
            extra_remaining_accounts: vec![AccountMeta::new(result_pda, false)],
        },
    )
}

// ── Payload Encoding ────────────────────────────────────────────────────

/// Encode a `GmpSolanaPayload` for `test_gmp_app::increment`.
///
/// Target accounts layout:
/// - `[0]`: `CounterAppState` (mutable)
/// - `[1]`: `UserCounter` PDA (mutable, `init_if_needed`)
/// - `[2]`: GMP account PDA (`user_authority`, signer via `invoke_signed`)
/// - `[3]`: GMP account PDA (payer, signer + mutable)
/// - `[4]`: system program
pub fn encode_increment_payload(
    counter_app_state: Pubkey,
    user_counter: Pubkey,
    gmp_account_pda: Pubkey,
    amount: u64,
) -> RawGmpSolanaPayload {
    let mut ix_data = anchor_discriminator("increment").to_vec();
    ix_data.extend_from_slice(&amount.to_le_bytes());

    RawGmpSolanaPayload {
        data: ix_data,
        accounts: vec![
            RawSolanaAccountMeta {
                pubkey: counter_app_state.to_bytes().to_vec(),
                is_signer: false,
                is_writable: true,
            },
            RawSolanaAccountMeta {
                pubkey: user_counter.to_bytes().to_vec(),
                is_signer: false,
                is_writable: true,
            },
            RawSolanaAccountMeta {
                pubkey: gmp_account_pda.to_bytes().to_vec(),
                is_signer: true,
                is_writable: false,
            },
            RawSolanaAccountMeta {
                pubkey: gmp_account_pda.to_bytes().to_vec(),
                is_signer: true,
                is_writable: true,
            },
            RawSolanaAccountMeta {
                pubkey: system_program::ID.to_bytes().to_vec(),
                is_signer: false,
                is_writable: false,
            },
        ],
        prefund_lamports: 5_000_000,
    }
}

/// Encode a full GMP packet wrapping a `GmpSolanaPayload`.
pub fn encode_gmp_packet(
    sender: &Pubkey,
    receiver: &Pubkey,
    solana_payload: &RawGmpSolanaPayload,
) -> Vec<u8> {
    let raw_packet = RawGmpPacketData {
        sender: sender.to_string(),
        receiver: receiver.to_string(),
        salt: vec![],
        payload: solana_payload.encode_to_vec(),
        memo: String::new(),
    };

    ics27_gmp::encoding::encode_gmp_packet(
        solana_ibc_types::GmpPacketData::try_from(raw_packet).expect("valid GMP packet data"),
        ICS27_ENCODING_PROTOBUF,
    )
    .expect("GMP packet encoding should succeed")
}

/// Build `remaining_accounts` entries for GMP `recv_packet` targeting
/// `test_gmp_app::increment`.
pub fn build_increment_remaining_accounts(
    gmp_account_pda: Pubkey,
    counter_app_state: Pubkey,
    user_counter: Pubkey,
) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new(gmp_account_pda, false),
        AccountMeta::new_readonly(test_gmp_app::ID, false),
        AccountMeta::new(counter_app_state, false),
        AccountMeta::new(user_counter, false),
        AccountMeta::new(gmp_account_pda, false),
        AccountMeta::new(gmp_account_pda, false),
        AccountMeta::new_readonly(system_program::ID, false),
    ]
}

/// Derive the GMP account PDA for a given sender and `client_id`.
pub fn derive_gmp_account_pda(client_id: &str, sender: &Pubkey) -> Pubkey {
    let gmp_account = solana_ibc_types::GMPAccount::new(
        client_id.to_string().try_into().expect("valid client_id"),
        sender.to_string().try_into().expect("valid sender"),
        vec![].try_into().expect("empty salt"),
        &ics27_gmp::ID,
    );
    gmp_account.pda
}

/// Derive the `UserCounter` PDA for `test_gmp_app`.
pub fn derive_user_counter_pda(user: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[test_gmp_app::state::UserCounter::SEED, user.as_ref()],
        &test_gmp_app::ID,
    );
    pda
}

/// Build IBC payload for GMP (used in `MsgPayload`).
pub fn build_gmp_ibc_payload(packet_bytes: &[u8]) -> Payload {
    Payload {
        source_port: GMP_PORT_ID.to_string(),
        dest_port: GMP_PORT_ID.to_string(),
        version: ICS27_VERSION.to_string(),
        encoding: ICS27_ENCODING_PROTOBUF.to_string(),
        value: packet_bytes.to_vec(),
    }
}
