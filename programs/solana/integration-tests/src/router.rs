use crate::accounts::anchor_discriminator;
use crate::chain::ChainAccounts;
use anchor_lang::{AnchorSerialize, InstructionData};
use ics26_router::state::*;
use solana_ibc_types::{
    Delivery, MsgAckPacket, MsgCleanupChunks, MsgPacket, MsgPayload, MsgProof, MsgTimeoutPacket,
    MsgUploadChunk, Payload,
};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
};

pub const PORT_ID: &str = "transfer";
pub const DEST_PORT: &str = "transfer";
pub const PROOF_HEIGHT: u64 = 100;

pub const fn test_timeout(clock_time: i64) -> u64 {
    clock_time as u64 + 86_000
}

pub fn derive_receipt_pda(dest_client: &str, sequence: u64) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_RECEIPT_SEED,
            dest_client.as_bytes(),
            &sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );
    pda
}

// ── Send ────────────────────────────────────────────────────────────────

pub struct SendPacketParams<'a> {
    pub sequence: u64,
    pub packet_data: &'a [u8],
}

pub struct SendResult {
    pub ix: Instruction,
    pub commitment_pda: Pubkey,
    pub packet: Packet,
}

pub fn build_send_packet_ix(
    user: Pubkey,
    accounts: &ChainAccounts,
    client_id: &str,
    counterparty_client_id: &str,
    clock_time: i64,
    params: SendPacketParams<'_>,
) -> SendResult {
    let timeout = test_timeout(clock_time);

    let (app_state_pda, _) =
        Pubkey::find_program_address(&[solana_ibc_types::IBCAppState::SEED], &test_ibc_app::ID);
    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (ibc_app_pda, _) =
        Pubkey::find_program_address(&[IBCApp::SEED, PORT_ID.as_bytes()], &ics26_router::ID);
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, client_id.as_bytes()], &ics26_router::ID);
    let (commitment_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            client_id.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );

    let msg = test_ibc_app::instructions::SendPacketMsg {
        source_client: client_id.to_string(),
        source_port: PORT_ID.to_string(),
        dest_port: DEST_PORT.to_string(),
        version: "1".to_string(),
        encoding: "json".to_string(),
        packet_data: params.packet_data.to_vec(),
        timeout_timestamp: timeout,
        sequence: params.sequence,
    };
    let mut data = anchor_discriminator("send_packet").to_vec();
    msg.serialize(&mut data).unwrap();

    let ix = Instruction {
        program_id: test_ibc_app::ID,
        accounts: vec![
            AccountMeta::new(app_state_pda, false),
            AccountMeta::new(user, true),
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new_readonly(ibc_app_pda, false),
            AccountMeta::new(commitment_pda, false),
            AccountMeta::new_readonly(client_pda, false),
            AccountMeta::new_readonly(mock_light_client::ID, false),
            AccountMeta::new_readonly(accounts.mock_client_state, false),
            AccountMeta::new_readonly(accounts.mock_consensus_state, false),
            AccountMeta::new_readonly(ics26_router::ID, false),
            AccountMeta::new_readonly(system_program::ID, false),
        ],
        data,
    };

    let packet = Packet {
        sequence: params.sequence,
        source_client: client_id.to_string(),
        dest_client: counterparty_client_id.to_string(),
        timeout_timestamp: timeout,
        payloads: vec![Payload {
            source_port: PORT_ID.to_string(),
            dest_port: DEST_PORT.to_string(),
            version: "1".to_string(),
            encoding: "json".to_string(),
            value: params.packet_data.to_vec(),
        }],
    };

    SendResult {
        ix,
        commitment_pda,
        packet,
    }
}

// ── Recv ────────────────────────────────────────────────────────────────

pub struct RecvPacketParams<'a> {
    pub sequence: u64,
    pub payload_chunk_pda: Pubkey,
    pub proof_chunk_pda: Pubkey,
    pub port_id: &'a str,
    pub version: &'a str,
    pub encoding: &'a str,
    pub app_program: Pubkey,
    pub extra_remaining_accounts: Vec<AccountMeta>,
}

#[derive(Debug)]
pub struct RecvResult {
    pub ix: Instruction,
    pub receipt_pda: Pubkey,
    pub ack_pda: Pubkey,
}

pub fn build_recv_packet_ix(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    dest_client: &str,
    source_client: &str,
    clock_time: i64,
    params: RecvPacketParams<'_>,
) -> RecvResult {
    let timeout = test_timeout(clock_time);

    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    let (ibc_app_pda, _) = Pubkey::find_program_address(
        &[IBCApp::SEED, params.port_id.as_bytes()],
        &ics26_router::ID,
    );
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, dest_client.as_bytes()], &ics26_router::ID);
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_RECEIPT_SEED,
            dest_client.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );
    let (ack_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_ACK_SEED,
            dest_client.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );

    let msg = MsgRecvPacket {
        packet: MsgPacket {
            sequence: params.sequence,
            source_client: source_client.to_string(),
            dest_client: dest_client.to_string(),
            timeout_timestamp: timeout,
            payloads: vec![MsgPayload {
                source_port: params.port_id.to_string(),
                dest_port: params.port_id.to_string(),
                version: params.version.to_string(),
                encoding: params.encoding.to_string(),
                data: Delivery::Chunked { total_chunks: 1 },
            }],
        },
        proof: MsgProof {
            height: PROOF_HEIGHT,
            data: Delivery::Chunked { total_chunks: 1 },
        },
    };

    let mut account_metas = vec![
        AccountMeta::new_readonly(router_state_pda, false),
        AccountMeta::new_readonly(access_manager_pda, false),
        AccountMeta::new_readonly(ibc_app_pda, false),
        AccountMeta::new(receipt_pda, false),
        AccountMeta::new(ack_pda, false),
        AccountMeta::new_readonly(params.app_program, false),
        AccountMeta::new(accounts.app_state_pda, false),
        AccountMeta::new(relayer, true),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        AccountMeta::new_readonly(client_pda, false),
        AccountMeta::new_readonly(mock_light_client::ID, false),
        AccountMeta::new_readonly(accounts.mock_client_state, false),
        AccountMeta::new_readonly(accounts.mock_consensus_state, false),
        AccountMeta::new(params.payload_chunk_pda, false),
        AccountMeta::new(params.proof_chunk_pda, false),
    ];
    account_metas.extend(params.extra_remaining_accounts);

    let ix = Instruction {
        program_id: ics26_router::ID,
        accounts: account_metas,
        data: ics26_router::instruction::RecvPacket { msg }.data(),
    };

    RecvResult {
        ix,
        receipt_pda,
        ack_pda,
    }
}

/// Build a `recv_packet` instruction with multiple proof chunks.
pub fn build_recv_packet_ix_multi_proof(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    dest_client: &str,
    source_client: &str,
    clock_time: i64,
    params: RecvPacketParams<'_>,
    proof_chunk_pdas: &[Pubkey],
) -> RecvResult {
    let total_proof_chunks = proof_chunk_pdas.len() as u8;
    let timeout = test_timeout(clock_time);

    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    let (ibc_app_pda, _) = Pubkey::find_program_address(
        &[IBCApp::SEED, params.port_id.as_bytes()],
        &ics26_router::ID,
    );
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, dest_client.as_bytes()], &ics26_router::ID);
    let (receipt_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_RECEIPT_SEED,
            dest_client.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );
    let (ack_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_ACK_SEED,
            dest_client.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );

    let msg = MsgRecvPacket {
        packet: MsgPacket {
            sequence: params.sequence,
            source_client: source_client.to_string(),
            dest_client: dest_client.to_string(),
            timeout_timestamp: timeout,
            payloads: vec![MsgPayload {
                source_port: params.port_id.to_string(),
                dest_port: params.port_id.to_string(),
                version: params.version.to_string(),
                encoding: params.encoding.to_string(),
                data: Delivery::Chunked { total_chunks: 1 },
            }],
        },
        proof: MsgProof {
            height: PROOF_HEIGHT,
            data: Delivery::Chunked {
                total_chunks: total_proof_chunks,
            },
        },
    };

    let mut account_metas = vec![
        AccountMeta::new_readonly(router_state_pda, false),
        AccountMeta::new_readonly(access_manager_pda, false),
        AccountMeta::new_readonly(ibc_app_pda, false),
        AccountMeta::new(receipt_pda, false),
        AccountMeta::new(ack_pda, false),
        AccountMeta::new_readonly(params.app_program, false),
        AccountMeta::new(accounts.app_state_pda, false),
        AccountMeta::new(relayer, true),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        AccountMeta::new_readonly(client_pda, false),
        AccountMeta::new_readonly(mock_light_client::ID, false),
        AccountMeta::new_readonly(accounts.mock_client_state, false),
        AccountMeta::new_readonly(accounts.mock_consensus_state, false),
        AccountMeta::new(params.payload_chunk_pda, false),
    ];
    for pda in proof_chunk_pdas {
        account_metas.push(AccountMeta::new(*pda, false));
    }
    account_metas.extend(params.extra_remaining_accounts);

    let ix = Instruction {
        program_id: ics26_router::ID,
        accounts: account_metas,
        data: ics26_router::instruction::RecvPacket { msg }.data(),
    };

    RecvResult {
        ix,
        receipt_pda,
        ack_pda,
    }
}

// ── Ack ─────────────────────────────────────────────────────────────────

pub struct AckPacketParams<'a> {
    pub sequence: u64,
    pub acknowledgement: Vec<u8>,
    pub payload_chunk_pda: Pubkey,
    pub proof_chunk_pda: Pubkey,
    pub port_id: &'a str,
    pub version: &'a str,
    pub encoding: &'a str,
    pub app_program: Pubkey,
    pub extra_remaining_accounts: Vec<AccountMeta>,
}

pub fn build_ack_packet_ix(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    source_client: &str,
    dest_client: &str,
    clock_time: i64,
    params: AckPacketParams<'_>,
) -> (Instruction, Pubkey) {
    let timeout = test_timeout(clock_time);

    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    let (ibc_app_pda, _) = Pubkey::find_program_address(
        &[IBCApp::SEED, params.port_id.as_bytes()],
        &ics26_router::ID,
    );
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, source_client.as_bytes()], &ics26_router::ID);
    let (commitment_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            source_client.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );

    let msg = MsgAckPacket {
        packet: MsgPacket {
            sequence: params.sequence,
            source_client: source_client.to_string(),
            dest_client: dest_client.to_string(),
            timeout_timestamp: timeout,
            payloads: vec![MsgPayload {
                source_port: params.port_id.to_string(),
                dest_port: params.port_id.to_string(),
                version: params.version.to_string(),
                encoding: params.encoding.to_string(),
                data: Delivery::Chunked { total_chunks: 1 },
            }],
        },
        acknowledgement: params.acknowledgement,
        proof: MsgProof {
            height: PROOF_HEIGHT,
            data: Delivery::Chunked { total_chunks: 1 },
        },
    };

    let mut account_metas = vec![
        AccountMeta::new_readonly(router_state_pda, false),
        AccountMeta::new_readonly(access_manager_pda, false),
        AccountMeta::new_readonly(ibc_app_pda, false),
        AccountMeta::new(commitment_pda, false),
        AccountMeta::new_readonly(params.app_program, false),
        AccountMeta::new(accounts.app_state_pda, false),
        AccountMeta::new(relayer, true),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        AccountMeta::new_readonly(client_pda, false),
        AccountMeta::new_readonly(mock_light_client::ID, false),
        AccountMeta::new_readonly(accounts.mock_client_state, false),
        AccountMeta::new_readonly(accounts.mock_consensus_state, false),
        AccountMeta::new(params.payload_chunk_pda, false),
        AccountMeta::new(params.proof_chunk_pda, false),
    ];
    account_metas.extend(params.extra_remaining_accounts);

    let ix = Instruction {
        program_id: ics26_router::ID,
        accounts: account_metas,
        data: ics26_router::instruction::AckPacket { msg }.data(),
    };

    (ix, commitment_pda)
}

/// Build an `ack_packet` instruction with multiple proof chunks.
pub fn build_ack_packet_ix_multi_proof(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    source_client: &str,
    dest_client: &str,
    clock_time: i64,
    params: AckPacketParams<'_>,
    proof_chunk_pdas: &[Pubkey],
) -> (Instruction, Pubkey) {
    let total_proof_chunks = proof_chunk_pdas.len() as u8;
    let timeout = test_timeout(clock_time);

    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    let (ibc_app_pda, _) = Pubkey::find_program_address(
        &[IBCApp::SEED, params.port_id.as_bytes()],
        &ics26_router::ID,
    );
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, source_client.as_bytes()], &ics26_router::ID);
    let (commitment_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            source_client.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );

    let msg = MsgAckPacket {
        packet: MsgPacket {
            sequence: params.sequence,
            source_client: source_client.to_string(),
            dest_client: dest_client.to_string(),
            timeout_timestamp: timeout,
            payloads: vec![MsgPayload {
                source_port: params.port_id.to_string(),
                dest_port: params.port_id.to_string(),
                version: params.version.to_string(),
                encoding: params.encoding.to_string(),
                data: Delivery::Chunked { total_chunks: 1 },
            }],
        },
        acknowledgement: params.acknowledgement,
        proof: MsgProof {
            height: PROOF_HEIGHT,
            data: Delivery::Chunked {
                total_chunks: total_proof_chunks,
            },
        },
    };

    let mut account_metas = vec![
        AccountMeta::new_readonly(router_state_pda, false),
        AccountMeta::new_readonly(access_manager_pda, false),
        AccountMeta::new_readonly(ibc_app_pda, false),
        AccountMeta::new(commitment_pda, false),
        AccountMeta::new_readonly(params.app_program, false),
        AccountMeta::new(accounts.app_state_pda, false),
        AccountMeta::new(relayer, true),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        AccountMeta::new_readonly(client_pda, false),
        AccountMeta::new_readonly(mock_light_client::ID, false),
        AccountMeta::new_readonly(accounts.mock_client_state, false),
        AccountMeta::new_readonly(accounts.mock_consensus_state, false),
        AccountMeta::new(params.payload_chunk_pda, false),
    ];
    for pda in proof_chunk_pdas {
        account_metas.push(AccountMeta::new(*pda, false));
    }
    account_metas.extend(params.extra_remaining_accounts);

    let ix = Instruction {
        program_id: ics26_router::ID,
        accounts: account_metas,
        data: ics26_router::instruction::AckPacket { msg }.data(),
    };

    (ix, commitment_pda)
}

// ── Timeout ────────────────────────────────────────────────────────────

pub struct TimeoutPacketParams<'a> {
    pub sequence: u64,
    pub payload_chunk_pda: Pubkey,
    pub proof_chunk_pda: Pubkey,
    pub port_id: &'a str,
    pub version: &'a str,
    pub encoding: &'a str,
    pub app_program: Pubkey,
    pub extra_remaining_accounts: Vec<AccountMeta>,
}

pub fn build_timeout_packet_ix(
    relayer: Pubkey,
    accounts: &ChainAccounts,
    source_client: &str,
    dest_client: &str,
    clock_time: i64,
    params: TimeoutPacketParams<'_>,
) -> (Instruction, Pubkey) {
    let timeout = test_timeout(clock_time);

    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    let (ibc_app_pda, _) = Pubkey::find_program_address(
        &[IBCApp::SEED, params.port_id.as_bytes()],
        &ics26_router::ID,
    );
    let (client_pda, _) =
        Pubkey::find_program_address(&[Client::SEED, source_client.as_bytes()], &ics26_router::ID);
    let (commitment_pda, _) = Pubkey::find_program_address(
        &[
            Commitment::PACKET_COMMITMENT_SEED,
            source_client.as_bytes(),
            &params.sequence.to_le_bytes(),
        ],
        &ics26_router::ID,
    );

    let msg = MsgTimeoutPacket {
        packet: MsgPacket {
            sequence: params.sequence,
            source_client: source_client.to_string(),
            dest_client: dest_client.to_string(),
            timeout_timestamp: timeout,
            payloads: vec![MsgPayload {
                source_port: params.port_id.to_string(),
                dest_port: params.port_id.to_string(),
                version: params.version.to_string(),
                encoding: params.encoding.to_string(),
                data: Delivery::Chunked { total_chunks: 1 },
            }],
        },
        proof: MsgProof {
            height: PROOF_HEIGHT,
            data: Delivery::Chunked { total_chunks: 1 },
        },
    };

    let mut account_metas = vec![
        AccountMeta::new_readonly(router_state_pda, false),
        AccountMeta::new_readonly(access_manager_pda, false),
        AccountMeta::new_readonly(ibc_app_pda, false),
        AccountMeta::new(commitment_pda, false),
        AccountMeta::new_readonly(params.app_program, false),
        AccountMeta::new(accounts.app_state_pda, false),
        AccountMeta::new(relayer, true),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        AccountMeta::new_readonly(client_pda, false),
        AccountMeta::new_readonly(mock_light_client::ID, false),
        AccountMeta::new_readonly(accounts.mock_client_state, false),
        AccountMeta::new_readonly(accounts.mock_consensus_state, false),
        AccountMeta::new(params.payload_chunk_pda, false),
        AccountMeta::new(params.proof_chunk_pda, false),
    ];
    account_metas.extend(params.extra_remaining_accounts);

    let ix = Instruction {
        program_id: ics26_router::ID,
        accounts: account_metas,
        data: ics26_router::instruction::TimeoutPacket { msg }.data(),
    };

    (ix, commitment_pda)
}

// ── Upload Chunks ─────────────────────────────────────────────────────

pub fn build_upload_payload_chunk_ix(
    relayer: Pubkey,
    client_id: &str,
    sequence: u64,
    payload_data: Vec<u8>,
) -> (Instruction, Pubkey) {
    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    let (chunk_pda, _) = Pubkey::find_program_address(
        &[
            PayloadChunk::SEED,
            relayer.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[0], // payload_index
            &[0], // chunk_index
        ],
        &ics26_router::ID,
    );

    let msg = MsgUploadChunk {
        client_id: client_id.to_string(),
        sequence,
        payload_index: 0,
        chunk_index: 0,
        chunk_data: payload_data,
    };

    let ix = Instruction {
        program_id: ics26_router::ID,
        accounts: vec![
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(relayer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ics26_router::instruction::UploadPayloadChunk { msg }.data(),
    };

    (ix, chunk_pda)
}

pub fn build_upload_proof_chunk_ix(
    relayer: Pubkey,
    client_id: &str,
    sequence: u64,
    proof_data: Vec<u8>,
) -> (Instruction, Pubkey) {
    build_upload_proof_chunk_ix_at(relayer, client_id, sequence, 0, proof_data)
}

pub fn build_upload_proof_chunk_ix_at(
    relayer: Pubkey,
    client_id: &str,
    sequence: u64,
    chunk_index: u8,
    proof_data: Vec<u8>,
) -> (Instruction, Pubkey) {
    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);
    let (chunk_pda, _) = Pubkey::find_program_address(
        &[
            ProofChunk::SEED,
            relayer.as_ref(),
            client_id.as_bytes(),
            &sequence.to_le_bytes(),
            &[chunk_index],
        ],
        &ics26_router::ID,
    );

    let msg = MsgUploadChunk {
        client_id: client_id.to_string(),
        sequence,
        payload_index: 0,
        chunk_index,
        chunk_data: proof_data,
    };

    let ix = Instruction {
        program_id: ics26_router::ID,
        accounts: vec![
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new(chunk_pda, false),
            AccountMeta::new(relayer, true),
            AccountMeta::new_readonly(system_program::ID, false),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
        ],
        data: ics26_router::instruction::UploadProofChunk { msg }.data(),
    };

    (ix, chunk_pda)
}

// ── Cleanup Chunks ───────────────────────────────────────────────────

pub fn build_cleanup_chunks_ix(
    relayer: Pubkey,
    client_id: &str,
    sequence: u64,
    payload_chunk_pda: Pubkey,
    proof_chunk_pda: Pubkey,
) -> Instruction {
    let (router_state_pda, _) =
        Pubkey::find_program_address(&[RouterState::SEED], &ics26_router::ID);
    let (access_manager_pda, _) =
        solana_ibc_types::access_manager::AccessManager::pda(access_manager::ID);

    let msg = MsgCleanupChunks {
        client_id: client_id.to_string(),
        sequence,
        payload_chunks: vec![1],
        total_proof_chunks: 1,
    };

    Instruction {
        program_id: ics26_router::ID,
        accounts: vec![
            AccountMeta::new_readonly(router_state_pda, false),
            AccountMeta::new_readonly(access_manager_pda, false),
            AccountMeta::new(relayer, true),
            AccountMeta::new_readonly(solana_sdk::sysvar::instructions::ID, false),
            AccountMeta::new(payload_chunk_pda, false),
            AccountMeta::new(proof_chunk_pda, false),
        ],
        data: ics26_router::instruction::CleanupChunks { msg }.data(),
    }
}
