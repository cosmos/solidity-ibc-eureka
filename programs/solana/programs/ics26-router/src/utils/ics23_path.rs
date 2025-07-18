/// Construct ICS24 commitment path for proof verification
/// Returns path segments: commitments/ports/{port_id}/channels/{dest_port}/sequences/{sequence}
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
/// Returns path segments: receipts/ports/{port_id}/channels/{dest_port}/sequences/{sequence}
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
/// Returns path segments: acks/ports/{port_id}/channels/{dest_port}/sequences/{sequence}
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
