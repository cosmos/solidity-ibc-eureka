use crate::errors::RouterError;
use crate::state::{Packet, Payload};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak::hash as keccak256;
use sha2::{Digest, Sha256};

/// Construct ICS24 commitment path for proof verification
/// Returns path segments: `commitments/ports/{port_id}/channels/{dest_port}/sequences/{sequence`}
pub fn construct_commitment_path(sequence: u64, port_id: &str, dest_port: &str) -> Vec<Vec<u8>> {
    vec![
        b"commitments".to_vec(),
        b"ports".to_vec(),
        port_id.as_bytes().to_vec(),
        b"channels".to_vec(),
        dest_port.as_bytes().to_vec(),
        b"sequences".to_vec(),
        sequence.to_string().as_bytes().to_vec(),
    ]
}

/// Construct ICS24 receipt path for proof verification
/// Returns path segments: `receipts/ports/{port_id}/channels/{dest_port}/sequences/{sequence`}
pub fn construct_receipt_path(sequence: u64, port_id: &str, dest_port: &str) -> Vec<Vec<u8>> {
    vec![
        b"receipts".to_vec(),
        b"ports".to_vec(),
        port_id.as_bytes().to_vec(),
        b"channels".to_vec(),
        dest_port.as_bytes().to_vec(),
        b"sequences".to_vec(),
        sequence.to_string().as_bytes().to_vec(),
    ]
}

/// Construct ICS24 acknowledgement path for proof verification
/// Returns path segments: `acks/ports/{port_id}/channels/{dest_port}/sequences/{sequence`}
pub fn construct_ack_path(sequence: u64, port_id: &str, dest_port: &str) -> Vec<Vec<u8>> {
    vec![
        b"acks".to_vec(),
        b"ports".to_vec(),
        port_id.as_bytes().to_vec(),
        b"channels".to_vec(),
        dest_port.as_bytes().to_vec(),
        b"sequences".to_vec(),
        sequence.to_string().as_bytes().to_vec(),
    ]
}

pub fn packet_commitment_path(client_id: &str, sequence: u64) -> Vec<u8> {
    let mut path = Vec::new();
    path.extend_from_slice(client_id.as_bytes());
    path.push(1u8);
    path.extend_from_slice(&sequence.to_be_bytes());
    path
}

pub fn packet_acknowledgement_commitment_path(client_id: &str, sequence: u64) -> Vec<u8> {
    let mut path = Vec::new();
    path.extend_from_slice(client_id.as_bytes());
    path.push(3u8);
    path.extend_from_slice(&sequence.to_be_bytes());
    path
}

pub fn packet_receipt_commitment_path(client_id: &str, sequence: u64) -> Vec<u8> {
    let mut path = Vec::new();
    path.extend_from_slice(client_id.as_bytes());
    path.push(2u8);
    path.extend_from_slice(&sequence.to_be_bytes());
    path
}

pub fn packet_commitment_key(client_id: &str, sequence: u64) -> [u8; 32] {
    let path = packet_commitment_path(client_id, sequence);
    keccak256(&path).to_bytes()
}

pub fn packet_acknowledgement_commitment_key(client_id: &str, sequence: u64) -> [u8; 32] {
    let path = packet_acknowledgement_commitment_path(client_id, sequence);
    keccak256(&path).to_bytes()
}

pub fn packet_receipt_commitment_key(client_id: &str, sequence: u64) -> [u8; 32] {
    let path = packet_receipt_commitment_path(client_id, sequence);
    keccak256(&path).to_bytes()
}

/// `sha256_hash(0x02` + `sha256_hash(destinationClient)` + `sha256_hash(timeout)` + `sha256_hash(payload)`)
pub fn packet_commitment_bytes32(packet: &Packet) -> [u8; 32] {
    let mut app_bytes = Vec::new();

    for payload in &packet.payloads {
        let payload_hash = hash_payload(payload);
        app_bytes.extend_from_slice(&payload_hash);
    }

    let mut hasher = Sha256::new();
    hasher.update([2u8]); // version byte
    hasher.update(sha256(packet.dest_client.as_bytes()));
    hasher.update(sha256(&packet.timeout_timestamp.to_be_bytes()));
    hasher.update(sha256(&app_bytes));

    hasher.finalize().into()
}

fn hash_payload(payload: &Payload) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(&sha256(payload.source_port.as_bytes()));
    buf.extend_from_slice(&sha256(payload.dest_port.as_bytes()));
    buf.extend_from_slice(&sha256(payload.version.as_bytes()));
    buf.extend_from_slice(&sha256(payload.encoding.as_bytes()));
    buf.extend_from_slice(&sha256(&payload.value));

    sha256(&buf)
}

/// `sha256_hash(0x02` + `sha256_hash(ack1)` + `sha256_hash(ack2)`, ...)
pub fn packet_acknowledgement_commitment_bytes32(acks: &[Vec<u8>]) -> Result<[u8; 32]> {
    require!(!acks.is_empty(), RouterError::NoAcknowledgements);

    let mut ack_bytes = Vec::new();
    for ack in acks {
        ack_bytes.extend_from_slice(&sha256(ack));
    }

    let mut hasher = Sha256::new();
    hasher.update([2u8]); // version byte
    hasher.update(&ack_bytes);

    Ok(hasher.finalize().into())
}

/// keccak256 hash of the packet
pub fn packet_receipt_commitment_bytes32(packet: &Packet) -> [u8; 32] {
    // Serialize packet deterministically
    let packet_bytes = packet.try_to_vec().expect("Failed to serialize packet");
    keccak256(&packet_bytes).to_bytes()
}

pub fn prefixed_path(merkle_prefix: &[Vec<u8>], path: &[u8]) -> Result<Vec<Vec<u8>>> {
    require!(!merkle_prefix.is_empty(), RouterError::InvalidMerklePrefix);

    let mut result = merkle_prefix.to_vec();
    let last_idx = result.len() - 1;
    result[last_idx].extend_from_slice(path);

    Ok(result)
}

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

