use crate::error::ErrorCode;
use crate::helpers::{
    decode_packet_attestation, deserialize_membership_proof, hash_path, verify_attestation,
};
use crate::VerifyMembership;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;
use ics25_handler::MembershipMsg;

pub fn verify_membership(ctx: Context<VerifyMembership>, msg: MembershipMsg) -> Result<()> {
    require!(!msg.value.is_empty(), ErrorCode::EmptyValue);
    require!(msg.path.len() == 1, ErrorCode::InvalidPathLength);

    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    require!(!client_state.is_frozen, ErrorCode::ClientFrozen);

    let proof = deserialize_membership_proof(&msg.proof)?;

    let attestation = decode_packet_attestation(&proof.attestation_data)?;

    require!(
        attestation.height == consensus_state_store.consensus_state.height,
        ErrorCode::HeightMismatch
    );

    verify_attestation(client_state, &proof.attestation_data, &proof.signatures)?;

    require!(!attestation.packets.is_empty(), ErrorCode::EmptyAttestation);

    let path_hash = hash_path(&msg.path[0]);

    let packet = attestation
        .packets
        .iter()
        .find(|p| p.path == path_hash)
        .ok_or(error!(ErrorCode::PathNotFound))?;

    let value_hash: [u8; 32] = msg
        .value
        .as_slice()
        .try_into()
        .map_err(|_| error!(ErrorCode::CommitmentMismatch))?;

    require!(
        packet.commitment == value_hash,
        ErrorCode::CommitmentMismatch
    );

    // Return the timestamp as required by the router CPI interface (matches Solidity behavior)
    let timestamp_bytes = consensus_state_store
        .consensus_state
        .timestamp
        .to_le_bytes();
    set_return_data(&timestamp_bytes);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::accounts::{
        create_client_state_account, create_consensus_state_account,
    };
    use crate::test_helpers::fixtures::{
        default_client_state, default_consensus_state, DEFAULT_CLIENT_ID,
    };
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use crate::types::{ClientState, ConsensusState, MembershipProof};
    use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
    use anchor_lang::InstructionData;
    use borsh::BorshSerialize;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::pubkey::Pubkey;

    const HEIGHT: u64 = 100;

    struct TestAccounts {
        client_state_pda: Pubkey,
        consensus_state_pda: Pubkey,
        accounts: Vec<(Pubkey, Account)>,
    }

    fn setup_test_accounts(
        client_id: &str,
        height: u64,
        client_state: ClientState,
        consensus_state: ConsensusState,
    ) -> TestAccounts {
        let client_state_pda = ClientState::pda(client_id);
        let consensus_state_pda = ConsensusStateStore::pda(&client_state_pda, height);

        let accounts = vec![
            (client_state_pda, create_client_state_account(&client_state)),
            (
                consensus_state_pda,
                create_consensus_state_account(height, consensus_state),
            ),
        ];

        TestAccounts {
            client_state_pda,
            consensus_state_pda,
            accounts,
        }
    }

    fn setup_default_test_accounts(client_id: &str, height: u64) -> TestAccounts {
        setup_test_accounts(
            client_id,
            height,
            default_client_state(client_id, height),
            default_consensus_state(height),
        )
    }

    fn create_verify_membership_instruction(
        test_accounts: &TestAccounts,
        msg: MembershipMsg,
    ) -> Instruction {
        let instruction_data = crate::instruction::VerifyMembership { msg };

        Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(test_accounts.client_state_pda, false),
                AccountMeta::new_readonly(test_accounts.consensus_state_pda, false),
            ],
            data: instruction_data.data(),
        }
    }

    #[test]
    fn test_verify_membership_empty_value() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"test/path".to_vec()],
            value: vec![],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::EmptyValue).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_invalid_path_length() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"path1".to_vec(), b"path2".to_vec()],
            value: vec![1, 2, 3],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidPathLength).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_frozen_client() {
        let mut client_state = default_client_state(DEFAULT_CLIENT_ID, HEIGHT);
        client_state.is_frozen = true;

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"test/path".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::ClientFrozen).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_invalid_proof() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![0xFF; 100],
            path: vec![b"test/path".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        // Garbage data may cause program crash or specific error - both are acceptable
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail with invalid proof data"
        );
    }

    #[test]
    fn test_verify_membership_empty_path() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidPathLength).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_height_mismatch() {
        let wrong_height = 200u64;
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let attestation_data =
            crate::test_helpers::fixtures::encode_packet_attestation(wrong_height, &[]);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::HeightMismatch).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_no_signatures() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[([1u8; 32], [2u8; 32])],
        );

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::NoSignatures).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_value_wrong_length() {
        let client_state = crate::test_helpers::fixtures::create_test_client_state(
            DEFAULT_CLIENT_ID,
            vec![],
            0,
            HEIGHT,
        );
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"test/path";
        let path_hash = crate::helpers::hash_path(path);
        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, [2u8; 32])],
        );

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![vec![0u8; 65]],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
            value: vec![1; 31],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail"
        );
    }

    #[test]
    fn test_verify_membership_empty_attestation_packets() {
        let client_state = crate::test_helpers::fixtures::create_test_client_state(
            DEFAULT_CLIENT_ID,
            vec![],
            0,
            HEIGHT,
        );
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let attestation_data =
            crate::test_helpers::fixtures::encode_packet_attestation(HEIGHT, &[]);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![vec![0u8; 65]],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail for empty attestation"
        );
    }

    #[test]
    fn test_verify_membership_three_paths() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"path1".to_vec(), b"path2".to_vec(), b"path3".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidPathLength).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_attestation_data_too_short() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let proof = MembershipProof {
            attestation_data: vec![0u8; 64],
            signatures: vec![vec![0u8; 65]],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail for short attestation data"
        );
    }

    #[test]
    fn test_verify_membership_invalid_signature_length() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let path = b"test/path";
        let path_hash = crate::helpers::hash_path(path);
        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, [2u8; 32])],
        );

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![vec![0u8; 64]],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
            value: vec![2; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidSignature).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_too_few_signatures() {
        let client_state = crate::test_helpers::fixtures::create_test_client_state(
            DEFAULT_CLIENT_ID,
            vec![[1u8; 20], [2u8; 20], [3u8; 20]],
            3,
            HEIGHT,
        );
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"test/path";
        let path_hash = crate::helpers::hash_path(path);
        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, [2u8; 32])],
        );

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![vec![1u8; 65], vec![2u8; 65]],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
            value: vec![2; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::TooFewSignatures).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_duplicate_signatures() {
        let client_state = crate::test_helpers::fixtures::create_test_client_state(
            DEFAULT_CLIENT_ID,
            vec![[1u8; 20], [2u8; 20]],
            2,
            HEIGHT,
        );
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"test/path";
        let path_hash = crate::helpers::hash_path(path);
        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, [2u8; 32])],
        );

        let sig = vec![1u8; 65];
        let proof = MembershipProof {
            attestation_data,
            signatures: vec![sig.clone(), sig],
        };

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
            value: vec![2; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::DuplicateSignature).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_membership_large_value() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"test/path".to_vec()],
            value: vec![0xFF; 1000],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_verify_membership_very_long_path() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let long_path = vec![0xAB; 1000];
        let msg = MembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![long_path],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_verify_membership_max_height() {
        let height = u64::MAX;
        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            height,
            default_client_state(DEFAULT_CLIENT_ID, height),
            default_consensus_state(height),
        );

        let msg = MembershipMsg {
            height,
            proof: vec![0xFF; 100],
            path: vec![b"test/path".to_vec()],
            value: vec![1; 32],
        };

        let instruction = create_verify_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(result.program_result.is_err());
    }
}
