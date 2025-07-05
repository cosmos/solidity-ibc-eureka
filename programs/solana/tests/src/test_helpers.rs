use ibc_proto::ibc::lightclients::tendermint::v1::Header as RawHeader;
use ibc_proto::ibc::core::commitment::v1::MerkleProof as RawMerkleProof;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use prost::Message;

/// Creates a minimal valid protobuf-encoded Header for testing
pub fn create_test_header_bytes() -> Vec<u8> {
    // Create a minimal RawHeader that will pass basic validation
    let raw_header = RawHeader {
        signed_header: Some(Default::default()),
        validator_set: Some(Default::default()),
        trusted_height: Some(ibc_proto::ibc::core::client::v1::Height {
            revision_number: 0,
            revision_height: 1,
        }),
        trusted_validators: Some(Default::default()),
    };
    
    // Encode to protobuf bytes
    let mut buf = Vec::new();
    raw_header.encode(&mut buf).expect("encoding should succeed");
    buf
}

/// Creates a minimal valid protobuf-encoded MerkleProof for testing
pub fn create_test_merkle_proof_bytes() -> Vec<u8> {
    // Create a minimal RawMerkleProof
    let raw_proof = RawMerkleProof {
        proofs: vec![],
    };
    
    // Encode to protobuf bytes
    let mut buf = Vec::new();
    raw_proof.encode(&mut buf).expect("encoding should succeed");
    buf
}

/// Creates a minimal valid protobuf-encoded Misbehaviour for testing
pub fn create_test_misbehaviour_bytes() -> Vec<u8> {
    // Create a minimal RawMisbehaviour with two headers
    let raw_misbehaviour = RawMisbehaviour {
        client_id: "test-client".to_string(),
        header_1: Some(Default::default()),
        header_2: Some(Default::default()),
    };
    
    // Encode to protobuf bytes
    let mut buf = Vec::new();
    raw_misbehaviour.encode(&mut buf).expect("encoding should succeed");
    buf
}