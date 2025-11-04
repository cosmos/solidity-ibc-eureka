use crate::errors::RouterError;
use crate::state::Packet;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak::hash as keccak256;
use sha2::{Digest, Sha256};
use solana_ibc_types::Payload;

/// Universal error acknowledgement as defined in ICS-04
/// This is the keccak256 hash of the string "error"
pub const UNIVERSAL_ERROR_ACK: &[u8] = b"error";

// TODO: move to a shared crate
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

// TODO: maybe remove
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_commitment_path() {
        let client_id = "test-client";
        let sequence = 42u64;
        let path = packet_commitment_path(client_id, sequence);

        let expected = [client_id.as_bytes(), &[1u8], &sequence.to_be_bytes()].concat();

        assert_eq!(path, expected);
    }

    #[test]
    fn test_packet_acknowledgement_commitment_path() {
        let client_id = "test-client";
        let sequence = 42u64;
        let path = packet_acknowledgement_commitment_path(client_id, sequence);

        let expected = [client_id.as_bytes(), &[3u8], &sequence.to_be_bytes()].concat();

        assert_eq!(path, expected);
    }

    #[test]
    fn test_packet_receipt_commitment_path() {
        let client_id = "test-client";
        let sequence = 42u64;
        let path = packet_receipt_commitment_path(client_id, sequence);

        let expected = [client_id.as_bytes(), &[2u8], &sequence.to_be_bytes()].concat();

        assert_eq!(path, expected);
    }

    #[test]
    fn test_packet_commitment_key() {
        let client_id = "test-client";
        let sequence = 42u64;
        let key = packet_commitment_key(client_id, sequence);

        assert_eq!(key.len(), 32);

        let key2 = packet_commitment_key(client_id, sequence);
        assert_eq!(key, key2);

        let key3 = packet_commitment_key("different-client", sequence);
        assert_ne!(key, key3);
    }

    #[test]
    fn test_packet_commitment_bytes32() {
        let packet = Packet {
            source_client: "source-client".to_string(),
            dest_client: "dest-client".to_string(),
            sequence: 1,
            timeout_timestamp: 1000,
            payloads: vec![Payload {
                source_port: "source-port".to_string(),
                dest_port: "dest-port".to_string(),
                version: "v1".to_string(),
                encoding: "json".to_string(),
                value: vec![1, 2, 3, 4],
            }],
        };

        let commitment = packet_commitment_bytes32(&packet);

        assert_eq!(commitment.len(), 32);

        let commitment2 = packet_commitment_bytes32(&packet);
        assert_eq!(commitment, commitment2);

        let mut packet2 = packet;
        packet2.timeout_timestamp = 2000; // Change timeout instead of sequence
        let commitment3 = packet_commitment_bytes32(&packet2);
        assert_ne!(commitment, commitment3);
    }

    #[test]
    fn test_packet_commitment_bytes32_multiple_payloads() {
        let packet = Packet {
            source_client: "source-client".to_string(),
            dest_client: "dest-client".to_string(),
            sequence: 1,
            timeout_timestamp: 1000,
            payloads: vec![
                Payload {
                    source_port: "port1".to_string(),
                    dest_port: "port1".to_string(),
                    version: "v1".to_string(),
                    encoding: "json".to_string(),
                    value: vec![1, 2, 3],
                },
                Payload {
                    source_port: "port2".to_string(),
                    dest_port: "port2".to_string(),
                    version: "v2".to_string(),
                    encoding: "protobuf".to_string(),
                    value: vec![4, 5, 6],
                },
            ],
        };

        let commitment = packet_commitment_bytes32(&packet);
        assert_eq!(commitment.len(), 32);

        let mut packet2 = packet;
        packet2.payloads.reverse();
        let commitment2 = packet_commitment_bytes32(&packet2);
        assert_ne!(commitment, commitment2);
    }

    #[test]
    fn test_hash_payload() {
        let payload = Payload {
            source_port: "source-port".to_string(),
            dest_port: "dest-port".to_string(),
            version: "v1".to_string(),
            encoding: "json".to_string(),
            value: vec![1, 2, 3, 4],
        };

        let hash = hash_payload(&payload);
        assert_eq!(hash.len(), 32);

        let hash2 = hash_payload(&payload);
        assert_eq!(hash, hash2);

        let mut payload2 = payload;
        payload2.value = vec![5, 6, 7, 8];
        let hash3 = hash_payload(&payload2);
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_packet_acknowledgement_commitment_bytes32() {
        let acks = vec![vec![1, 2, 3], vec![4, 5, 6]];

        let commitment = packet_acknowledgement_commitment_bytes32(&acks).unwrap();
        assert_eq!(commitment.len(), 32);

        let commitment2 = packet_acknowledgement_commitment_bytes32(&acks).unwrap();
        assert_eq!(commitment, commitment2);

        let acks2 = vec![vec![4, 5, 6], vec![1, 2, 3]];
        let commitment3 = packet_acknowledgement_commitment_bytes32(&acks2).unwrap();
        assert_ne!(commitment, commitment3);
    }

    #[test]
    fn test_packet_acknowledgement_commitment_bytes32_empty() {
        let acks: Vec<Vec<u8>> = vec![];
        let result = packet_acknowledgement_commitment_bytes32(&acks);
        assert!(result.is_err());
    }

    #[test]
    fn test_packet_acknowledgement_commitment_bytes32_single() {
        let acks = vec![vec![1, 2, 3, 4, 5]];
        let commitment = packet_acknowledgement_commitment_bytes32(&acks).unwrap();
        assert_eq!(commitment.len(), 32);
    }

    #[test]
    fn test_packet_receipt_commitment_bytes32() {
        let packet = Packet {
            source_client: "source-client".to_string(),
            dest_client: "dest-client".to_string(),
            sequence: 1,
            timeout_timestamp: 1000,
            payloads: vec![Payload {
                source_port: "source-port".to_string(),
                dest_port: "dest-port".to_string(),
                version: "v1".to_string(),
                encoding: "json".to_string(),
                value: vec![1, 2, 3, 4],
            }],
        };

        let commitment = packet_receipt_commitment_bytes32(&packet);
        assert_eq!(commitment.len(), 32);

        let commitment2 = packet_receipt_commitment_bytes32(&packet);
        assert_eq!(commitment, commitment2);

        let mut packet2 = packet;
        packet2.sequence = 2;
        let commitment3 = packet_receipt_commitment_bytes32(&packet2);
        assert_ne!(commitment, commitment3);
    }

    #[test]
    fn test_prefixed_path() {
        let merkle_prefix = vec![vec![1, 2, 3], vec![4, 5, 6]];
        let path = vec![7, 8, 9];

        let result = prefixed_path(&merkle_prefix, &path).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], vec![1, 2, 3]);
        assert_eq!(result[1], vec![4, 5, 6, 7, 8, 9]); // Last element extended with path
    }

    #[test]
    fn test_prefixed_path_empty_prefix() {
        let merkle_prefix: Vec<Vec<u8>> = vec![];
        let path = vec![1, 2, 3];

        let result = prefixed_path(&merkle_prefix, &path);
        assert!(result.is_err());
    }

    #[test]
    fn test_prefixed_path_single_prefix() {
        let merkle_prefix = vec![vec![1, 2, 3]];
        let path = vec![4, 5, 6];

        let result = prefixed_path(&merkle_prefix, &path).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0], vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_sha256() {
        let data = b"hello world";
        let hash = sha256(data);

        // Verify it's a 32-byte hash
        assert_eq!(hash.len(), 32);

        // Verify it's deterministic
        let hash2 = sha256(data);
        assert_eq!(hash, hash2);

        // Verify different inputs produce different hashes
        let hash3 = sha256(b"different data");
        assert_ne!(hash, hash3);

        // Test empty input
        let hash_empty = sha256(b"");
        assert_eq!(hash_empty.len(), 32);
    }

    #[test]
    fn test_path_consistency() {
        let client_id = "test-client";
        let sequence = 100u64;

        // All paths should have consistent structure
        let commit_path = packet_commitment_path(client_id, sequence);
        let ack_path = packet_acknowledgement_commitment_path(client_id, sequence);
        let receipt_path = packet_receipt_commitment_path(client_id, sequence);

        // Same length (client_id + 1 byte separator + 8 bytes sequence)
        assert_eq!(commit_path.len(), client_id.len() + 1 + 8);
        assert_eq!(ack_path.len(), client_id.len() + 1 + 8);
        assert_eq!(receipt_path.len(), client_id.len() + 1 + 8);

        // Same prefix
        assert_eq!(&commit_path[..client_id.len()], client_id.as_bytes());
        assert_eq!(&ack_path[..client_id.len()], client_id.as_bytes());
        assert_eq!(&receipt_path[..client_id.len()], client_id.as_bytes());

        // Different separators
        assert_eq!(commit_path[client_id.len()], 1);
        assert_eq!(receipt_path[client_id.len()], 2);
        assert_eq!(ack_path[client_id.len()], 3);

        // Same sequence suffix
        let seq_bytes = &sequence.to_be_bytes();
        assert_eq!(&commit_path[client_id.len() + 1..], seq_bytes);
        assert_eq!(&ack_path[client_id.len() + 1..], seq_bytes);
        assert_eq!(&receipt_path[client_id.len() + 1..], seq_bytes);
    }
}
