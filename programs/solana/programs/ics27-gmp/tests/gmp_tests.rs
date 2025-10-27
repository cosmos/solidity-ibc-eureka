use ics27_gmp::{state::*, ID};
use solana_sdk::pubkey::Pubkey;

// ============================================================================
// Account State Tests
// ============================================================================

#[test]
fn test_account_state_derive_address() {
    let client_id = "cosmoshub-1";
    let sender = "cosmos1abc123def456";
    let salt = b"salt123";

    let (address1, bump1) = AccountState::derive_address(client_id, sender, salt, &ID).unwrap();
    let (address2, bump2) = AccountState::derive_address(client_id, sender, salt, &ID).unwrap();

    // Same inputs should produce same PDA
    assert_eq!(address1, address2);
    assert_eq!(bump1, bump2);

    // Different sender should produce different PDA
    let (address3, _) =
        AccountState::derive_address(client_id, "cosmos1different", salt, &ID).unwrap();
    assert_ne!(address1, address3);
}

#[test]
fn test_account_state_long_sender_hashed() {
    // Test with very long sender address (>32 bytes)
    let client_id = "cosmoshub-1";
    let long_sender = "cosmos1".to_string() + &"a".repeat(120);
    let salt = b"";

    // Should succeed by hashing the long sender
    let result = AccountState::derive_address(client_id, &long_sender, salt, &ID);
    assert!(result.is_ok(), "Should handle long sender by hashing");
}

#[test]
fn test_account_state_validation() {
    let client_id = "a".repeat(33); // Exceeds MAX_CLIENT_ID_LENGTH
    let sender = "cosmos1test";
    let salt = b"salt";

    let result = AccountState::derive_address(&client_id, sender, salt, &ID);
    assert!(result.is_err(), "Should fail with client ID too long");
}

// ============================================================================
// Packet Data Validation Tests
// ============================================================================

#[test]
fn test_gmp_packet_data_validation_success() {
    let packet_data = GMPPacketData {
        client_id: "cosmoshub-1".to_string(),
        sender: "cosmos1abc".to_string(),
        receiver: Pubkey::new_unique().to_string(),
        salt: vec![1, 2, 3],
        payload: vec![1, 2, 3, 4, 5],
        memo: "test memo".to_string(),
    };

    assert!(
        packet_data.validate().is_ok(),
        "Valid packet should pass validation"
    );
}

#[test]
fn test_gmp_packet_data_validation_empty_client_id() {
    let packet_data = GMPPacketData {
        client_id: String::new(),
        sender: "cosmos1abc".to_string(),
        receiver: Pubkey::new_unique().to_string(),
        salt: vec![],
        payload: vec![1, 2, 3],
        memo: String::new(),
    };

    assert!(
        packet_data.validate().is_err(),
        "Empty client ID should fail"
    );
}

#[test]
fn test_gmp_packet_data_validation_empty_sender() {
    let packet_data = GMPPacketData {
        client_id: "cosmoshub-1".to_string(),
        sender: String::new(),
        receiver: Pubkey::new_unique().to_string(),
        salt: vec![],
        payload: vec![1, 2, 3],
        memo: String::new(),
    };

    assert!(packet_data.validate().is_err(), "Empty sender should fail");
}

#[test]
fn test_gmp_packet_data_validation_empty_payload() {
    let packet_data = GMPPacketData {
        client_id: "cosmoshub-1".to_string(),
        sender: "cosmos1abc".to_string(),
        receiver: Pubkey::new_unique().to_string(),
        salt: vec![],
        payload: vec![],
        memo: String::new(),
    };

    assert!(packet_data.validate().is_err(), "Empty payload should fail");
}

#[test]
fn test_gmp_packet_data_validation_payload_too_long() {
    let packet_data = GMPPacketData {
        client_id: "cosmoshub-1".to_string(),
        sender: "cosmos1abc".to_string(),
        receiver: Pubkey::new_unique().to_string(),
        salt: vec![],
        payload: vec![0; 1025], // Exceeds MAX_PAYLOAD_LENGTH
        memo: String::new(),
    };

    assert!(
        packet_data.validate().is_err(),
        "Payload too long should fail"
    );
}

#[test]
fn test_gmp_packet_data_validation_memo_too_long() {
    let packet_data = GMPPacketData {
        client_id: "cosmoshub-1".to_string(),
        sender: "cosmos1abc".to_string(),
        receiver: Pubkey::new_unique().to_string(),
        salt: vec![],
        payload: vec![1, 2, 3],
        memo: "a".repeat(257), // Exceeds MAX_MEMO_LENGTH
    };

    assert!(packet_data.validate().is_err(), "Memo too long should fail");
}

#[test]
fn test_gmp_packet_data_validation_salt_too_long() {
    let packet_data = GMPPacketData {
        client_id: "cosmoshub-1".to_string(),
        sender: "cosmos1abc".to_string(),
        receiver: Pubkey::new_unique().to_string(),
        salt: vec![0; 9], // Exceeds MAX_SALT_LENGTH
        payload: vec![1, 2, 3],
        memo: String::new(),
    };

    assert!(packet_data.validate().is_err(), "Salt too long should fail");
}

// ============================================================================
// Send Call Message Validation Tests
// ============================================================================

#[test]
fn test_send_call_msg_validation_success() {
    let current_time = 1_000_000;
    let msg = SendCallMsg {
        source_client: "cosmoshub-1".to_string(),
        timeout_timestamp: current_time + 3600, // 1 hour from now
        receiver: Pubkey::new_unique(),
        salt: vec![1, 2, 3],
        payload: vec![1, 2, 3, 4, 5],
        memo: "test memo".to_string(),
    };

    assert!(
        msg.validate(current_time).is_ok(),
        "Valid message should pass validation"
    );
}

#[test]
fn test_send_call_msg_validation_timeout_too_long() {
    let current_time = 1_000_000;
    let msg = SendCallMsg {
        source_client: "cosmoshub-1".to_string(),
        timeout_timestamp: current_time + 90_000, // More than MAX_TIMEOUT_DURATION
        receiver: Pubkey::new_unique(),
        salt: vec![],
        payload: vec![1, 2, 3],
        memo: String::new(),
    };

    assert!(
        msg.validate(current_time).is_err(),
        "Timeout too long should fail"
    );
}

#[test]
fn test_send_call_msg_validation_timeout_too_soon() {
    let current_time = 1_000_000;
    let msg = SendCallMsg {
        source_client: "cosmoshub-1".to_string(),
        timeout_timestamp: current_time + 5, // Less than MIN_TIMEOUT_DURATION (12 seconds)
        receiver: Pubkey::new_unique(),
        salt: vec![],
        payload: vec![1, 2, 3],
        memo: String::new(),
    };

    assert!(
        msg.validate(current_time).is_err(),
        "Timeout too soon should fail"
    );
}

// ============================================================================
// Solana Instruction Extension Tests
// ============================================================================

#[test]
fn test_solana_instruction_validation_success() {
    let instruction = ics27_gmp::proto::SolanaInstruction {
        program_id: vec![1; 32],
        accounts: vec![ics27_gmp::proto::SolanaAccountMeta {
            pubkey: vec![2; 32],
            is_signer: true,
            is_writable: true,
        }],
        data: vec![1, 2, 3],
        payer_position: None,
    };

    assert!(
        instruction.validate().is_ok(),
        "Valid instruction should pass"
    );
}

#[test]
fn test_solana_instruction_validation_invalid_program_id() {
    let instruction = ics27_gmp::proto::SolanaInstruction {
        program_id: vec![1; 20], // Invalid length
        accounts: vec![],
        data: vec![1, 2, 3],
        payer_position: None,
    };

    assert!(
        instruction.validate().is_err(),
        "Invalid program ID should fail"
    );
}

#[test]
fn test_solana_instruction_validation_empty_data() {
    let instruction = ics27_gmp::proto::SolanaInstruction {
        program_id: vec![1; 32],
        accounts: vec![],
        data: vec![],
        payer_position: None,
    };

    assert!(instruction.validate().is_err(), "Empty data should fail");
}

#[test]
fn test_solana_instruction_validation_too_many_accounts() {
    let instruction = ics27_gmp::proto::SolanaInstruction {
        program_id: vec![1; 32],
        accounts: vec![
            ics27_gmp::proto::SolanaAccountMeta {
                pubkey: vec![2; 32],
                is_signer: false,
                is_writable: false,
            };
            33 // More than 32 accounts
        ],
        data: vec![1, 2, 3],
        payer_position: None,
    };

    assert!(
        instruction.validate().is_err(),
        "Too many accounts should fail"
    );
}

#[test]
fn test_solana_instruction_to_account_metas() {
    let instruction = ics27_gmp::proto::SolanaInstruction {
        program_id: vec![1; 32],
        accounts: vec![
            ics27_gmp::proto::SolanaAccountMeta {
                pubkey: Pubkey::new_unique().to_bytes().to_vec(),
                is_signer: true,
                is_writable: true,
            },
            ics27_gmp::proto::SolanaAccountMeta {
                pubkey: Pubkey::new_unique().to_bytes().to_vec(),
                is_signer: false,
                is_writable: false,
            },
        ],
        data: vec![1, 2, 3],
        payer_position: None,
    };

    let metas = instruction.to_account_metas().unwrap();
    assert_eq!(metas.len(), 2);
    assert!(metas[0].is_signer);
    assert!(metas[0].is_writable);
    assert!(!metas[1].is_signer);
    assert!(!metas[1].is_writable);
}

// ============================================================================
// GMP Acknowledgement Tests
// ============================================================================

#[test]
fn test_gmp_acknowledgement_success() {
    let data = vec![1, 2, 3, 4];
    let ack = ics27_gmp::proto::GmpAcknowledgement::success(data.clone());

    assert!(ack.success);
    assert_eq!(ack.data, data);
    assert!(ack.error.is_empty());
}

#[test]
fn test_gmp_acknowledgement_error() {
    let error_msg = "execution failed".to_string();
    let ack = ics27_gmp::proto::GmpAcknowledgement::error(error_msg.clone());

    assert!(!ack.success);
    assert!(ack.data.is_empty());
    assert_eq!(ack.error, error_msg);
}

#[test]
fn test_gmp_acknowledgement_serialization() {
    let ack = ics27_gmp::proto::GmpAcknowledgement::success(vec![1, 2, 3]);
    let serialized = ack.try_to_vec().unwrap();

    let deserialized = ics27_gmp::proto::GmpAcknowledgement::try_from_slice(&serialized).unwrap();
    assert_eq!(ack.success, deserialized.success);
    assert_eq!(ack.data, deserialized.data);
}

// ============================================================================
// App State Operation Tests
// ============================================================================

#[test]
fn test_app_state_can_operate_when_not_paused() {
    let router_program = Pubkey::new_unique();
    let authority = Pubkey::new_unique();
    let (_app_state_pda, bump) =
        Pubkey::find_program_address(&[ics27_gmp::constants::GMP_APP_STATE_SEED, b"gmpport"], &ID);

    let app_state = GMPAppState {
        router_program,
        authority,
        version: 1,
        paused: false,
        bump,
    };

    assert!(
        app_state.can_operate().is_ok(),
        "App should be operational when not paused"
    );
}

#[test]
fn test_app_state_cannot_operate_when_paused() {
    let router_program = Pubkey::new_unique();
    let authority = Pubkey::new_unique();
    let (_app_state_pda, bump) =
        Pubkey::find_program_address(&[ics27_gmp::constants::GMP_APP_STATE_SEED, b"gmpport"], &ID);

    let app_state = GMPAppState {
        router_program,
        authority,
        version: 1,
        paused: true, // Paused
        bump,
    };

    assert!(
        app_state.can_operate().is_err(),
        "App should not be operational when paused"
    );
}

#[test]
fn test_account_state_nonce_increment() {
    let (_account_pda, bump) =
        AccountState::derive_address("cosmoshub-1", "cosmos1test", b"", &ID).unwrap();

    let mut account_state = AccountState {
        client_id: "cosmoshub-1".to_string(),
        sender: "cosmos1test".to_string(),
        salt: vec![],
        nonce: 5,
        created_at: 1_600_000_000,
        last_executed_at: 1_600_000_000,
        execution_count: 10,
        bump,
    };

    let current_time = 1_700_000_000;
    account_state.execute_nonce_increment(current_time);

    assert_eq!(account_state.nonce, 6);
    assert_eq!(account_state.last_executed_at, current_time);
    assert_eq!(account_state.execution_count, 11);
}
