use crate::errors::GMPError;
use anchor_lang::prelude::*;
use solana_ibc_types::MsgSendPacket;
use solana_program::{hash::hash, instruction::Instruction, program::invoke_signed};

/// Send IBC packet via CPI to the ICS26 router
/// This function creates and sends a GMP packet from Solana to another chain
#[allow(clippy::too_many_arguments)]
pub fn send_packet_cpi<'a>(
    router_program: &AccountInfo<'a>,
    router_state: &AccountInfo<'a>,
    client_sequence: &AccountInfo<'a>,
    packet_commitment: &AccountInfo<'a>,
    router_caller: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    ibc_app: &AccountInfo<'a>,
    client: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    msg: MsgSendPacket,
    router_caller_bump: u8,
) -> Result<u64> {
    // Build instruction data with Anchor discriminator
    let mut instruction_data = Vec::with_capacity(256);

    // Anchor instruction discriminator: first 8 bytes of hash of "global:send_packet"
    let discriminator = hash(b"global:send_packet").to_bytes();
    instruction_data.extend_from_slice(&discriminator[..8]);

    // Append serialized MsgSendPacket data
    msg.serialize(&mut instruction_data)?;

    // Build CPI instruction for router's send_packet
    let instruction = Instruction {
        program_id: *router_program.key,
        accounts: vec![
            AccountMeta::new_readonly(*router_state.key, false),
            AccountMeta::new_readonly(*ibc_app.key, false),
            AccountMeta::new(*client_sequence.key, false),
            AccountMeta::new(*packet_commitment.key, false),
            AccountMeta::new_readonly(*router_caller.key, true), // GMP's router_caller PDA signs
            AccountMeta::new(*payer.key, true),
            AccountMeta::new_readonly(*system_program.key, false),
            AccountMeta::new_readonly(*client.key, false),
        ],
        data: instruction_data,
    };

    // Router caller PDA signer seeds
    let signer_seeds = &[b"router_caller".as_slice(), &[router_caller_bump]];

    // Execute CPI to router with PDA signing
    let account_infos = &[
        router_state.clone(),
        ibc_app.clone(),
        client_sequence.clone(),
        packet_commitment.clone(),
        router_caller.clone(),
        payer.clone(),
        system_program.clone(),
        client.clone(),
    ];

    invoke_signed(&instruction, account_infos, &[signer_seeds])?;

    // Read sequence number from updated client_sequence account
    // The router increments the sequence after sending the packet
    let client_sequence_data = client_sequence.try_borrow_data()?;
    if client_sequence_data.len() >= 16 {
        // Account layout: 8 bytes Anchor discriminator + 8 bytes u64 sequence
        let sequence_bytes = &client_sequence_data[8..16];
        let current_sequence = u64::from_le_bytes(
            sequence_bytes
                .try_into()
                .map_err(|_| GMPError::SequenceParseError)?,
        );
        // Return the sequence that was just used (current - 1)
        Ok(current_sequence.saturating_sub(1))
    } else {
        Err(GMPError::SequenceParseError.into())
    }
}

/// Parse GMP packet data from router CPI call
///
/// Extracts and validates `GMPPacketData` from the Protobuf-encoded IBC packet payload.
/// The router passes `OnRecvPacketMsg` which contains the source chain client ID and
/// Protobuf-encoded packet data. This function decodes the Protobuf payload and
/// combines it with the IBC context to create the full `GMPPacketData` structure.
///
/// Note: Port ID validation should be done by the caller using `app_state.port_id`
/// Note: Receiver is kept as string - caller must parse to Pubkey when needed (incoming packets)
pub fn parse_packet_data_from_router_cpi(
    msg: &solana_ibc_types::OnRecvPacketMsg,
) -> Result<Box<crate::state::GMPPacketData>> {
    // Decode Protobuf payload from IBC packet
    let proto_data = decode_gmp_packet_data(msg.payload.value.as_slice())?;

    // Construct full GMPPacketData by combining Protobuf data with IBC context
    let packet_data = Box::new(crate::state::GMPPacketData {
        client_id: msg.source_client.clone(), // From IBC context (e.g., "07-tendermint-0")
        sender: proto_data.sender,
        receiver: proto_data.receiver, // Keep as string (Solana Pubkey base58 for incoming packets)
        salt: proto_data.salt,
        payload: proto_data.payload, // SolanaInstruction (Protobuf-encoded)
        memo: proto_data.memo,
    });

    // Validate all fields (lengths, non-empty checks, etc.)
    packet_data.validate()?;

    Ok(packet_data)
}

/// Parse acknowledgement data from router CPI call
///
/// Extracts packet data and acknowledgement from `OnAcknowledgementPacketMsg`.
/// Used when the destination chain sends back an acknowledgement for a packet
/// that was previously sent from this Solana chain.
///
/// Note: For acks of outgoing packets, receiver is a Cosmos address or empty string
pub fn parse_ack_data_from_router_cpi(
    msg: &solana_ibc_types::OnAcknowledgementPacketMsg,
) -> Result<(Box<crate::state::GMPPacketData>, Vec<u8>)> {
    let proto_data = decode_gmp_packet_data(msg.payload.value.as_slice())?;

    let packet_data = Box::new(crate::state::GMPPacketData {
        client_id: msg.source_client.clone(),
        sender: proto_data.sender,
        receiver: proto_data.receiver, // Keep as string (Cosmos address for outgoing packets)
        salt: proto_data.salt,
        payload: proto_data.payload,
        memo: proto_data.memo,
    });

    packet_data.validate()?;

    // Return both the original packet data and the acknowledgement from destination chain
    Ok((packet_data, msg.acknowledgement.clone()))
}

/// Parse timeout data from router CPI call
///
/// Extracts packet data from `OnTimeoutPacketMsg` when a packet times out.
/// This occurs when the packet was not delivered to the destination chain
/// within the specified timeout period, and can be proven via timeout proof.
///
/// Note: For timeouts of outgoing packets, receiver is a Cosmos address or empty string
pub fn parse_timeout_data_from_router_cpi(
    msg: &solana_ibc_types::OnTimeoutPacketMsg,
) -> Result<Box<crate::state::GMPPacketData>> {
    // Decode original packet data that timed out
    let proto_data = decode_gmp_packet_data(msg.payload.value.as_slice())?;

    let packet_data = Box::new(crate::state::GMPPacketData {
        client_id: msg.source_client.clone(),
        sender: proto_data.sender,
        receiver: proto_data.receiver, // Keep as string (Cosmos address for outgoing packets)
        salt: proto_data.salt,
        payload: proto_data.payload,
        memo: proto_data.memo,
    });

    packet_data.validate()?;

    Ok(packet_data)
}

/// Decode GMP packet data from protobuf payload with error logging
fn decode_gmp_packet_data(payload: &[u8]) -> Result<crate::proto::GmpPacketData> {
    use prost::Message;
    crate::proto::GmpPacketData::decode(payload).map_err(|e| {
        msg!("Failed to decode GMP packet data: {}", e);
        GMPError::PacketDataParseError.into()
    })
}
