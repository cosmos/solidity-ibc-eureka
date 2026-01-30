use crate::abi_decode::decode_packet_attestation;
use crate::crypto::hash_path;
use crate::error::ErrorCode;
use crate::proof::deserialize_membership_proof;
use crate::state::ConsensusStateStore;
use crate::types::ClientState;
use crate::verification::verify_attestation;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;
use ics25_handler::NonMembershipMsg;

#[derive(Accounts)]
#[instruction(msg: ics25_handler::NonMembershipMsg)]
pub struct VerifyNonMembership<'info> {
    pub client_state: Account<'info, ClientState>,
    #[account(
        seeds = [ConsensusStateStore::SEED, client_state.key().as_ref(), &msg.height.to_le_bytes()],
        bump
    )]
    pub consensus_state_at_height: Account<'info, ConsensusStateStore>,
}

pub fn verify_non_membership(
    ctx: Context<VerifyNonMembership>,
    msg: NonMembershipMsg,
) -> Result<()> {
    require!(msg.path.len() == 1, ErrorCode::InvalidPathLength);

    let client_state = &ctx.accounts.client_state;
    let consensus_state_store = &ctx.accounts.consensus_state_at_height;

    require!(!client_state.is_frozen, ErrorCode::FrozenClientState);

    // Ensure we have a trusted timestamp at the provided height
    require!(
        consensus_state_store.consensus_state.timestamp != 0,
        ErrorCode::ConsensusTimestampNotFound
    );

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
        .ok_or_else(|| error!(ErrorCode::NotMember))?;

    require!(packet.commitment == [0u8; 32], ErrorCode::NonZeroCommitment);

    // The router calls this light client via CPI and reads the returned timestamp
    // via get_return_data() to perform timeout checks against packet expiration
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
    use crate::test_helpers::signing::TestAttestor;
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

    fn create_verify_non_membership_instruction(
        test_accounts: &TestAccounts,
        msg: NonMembershipMsg,
    ) -> Instruction {
        let instruction_data = crate::instruction::VerifyNonMembership { msg };

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
    fn test_verify_non_membership_invalid_path_length() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"path1".to_vec(), b"path2".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidPathLength).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_frozen_client() {
        let mut client_state = default_client_state(DEFAULT_CLIENT_ID, HEIGHT);
        client_state.is_frozen = true;

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::FrozenClientState).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_empty_path() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidPathLength).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_invalid_proof() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: vec![0xFF; 100],
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        // Garbage data may cause program crash or specific error - both are acceptable
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail with invalid proof data"
        );
    }

    #[test]
    fn test_verify_non_membership_height_mismatch() {
        let wrong_height = 200u64;
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let attestation_data =
            crate::test_helpers::fixtures::encode_packet_attestation(wrong_height, &[]);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::HeightMismatch).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_no_signatures() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[([0u8; 32], [0u8; 32])],
        );

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::EmptySignatures).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_empty_attestation() {
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![],
            min_required_sigs: 0,
            latest_height: HEIGHT,
            is_frozen: false,
        };

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

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail"
        );
    }

    #[test]
    fn test_verify_non_membership_too_few_signatures() {
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![[1u8; 20], [2u8; 20]],
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[([0u8; 32], [0u8; 32])],
        );

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![vec![0u8; 65]],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::ThresholdNotMet).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_duplicate_signers() {
        let attestor = TestAttestor::new(1);
        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[([0u8; 32], [0u8; 32])],
        );

        // Same attestor signs twice - recovers to same address
        let sig = attestor.sign(&attestation_data);
        let proof = MembershipProof {
            attestation_data,
            signatures: vec![sig.clone(), sig],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::DuplicateSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_three_paths() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"path1".to_vec(), b"path2".to_vec(), b"path3".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidPathLength).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_attestation_data_too_short() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let proof = MembershipProof {
            attestation_data: vec![0u8; 64],
            signatures: vec![vec![0u8; 65]],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(
            result.program_result.is_err(),
            "Expected instruction to fail for short attestation data"
        );
    }

    #[test]
    fn test_verify_non_membership_invalid_signature_length() {
        let test_accounts = setup_default_test_accounts(DEFAULT_CLIENT_ID, HEIGHT);

        let path = b"test/path";
        let path_hash = crate::crypto::hash_path(path);
        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, [0u8; 32])],
        );

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![vec![0u8; 64]],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::InvalidSignature).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_large_timestamp() {
        let consensus_state = ConsensusState {
            height: HEIGHT,
            timestamp: u64::MAX,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            default_client_state(DEFAULT_CLIENT_ID, HEIGHT),
            consensus_state,
        );

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: vec![0xFF; 100],
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let result = mollusk.process_instruction(&instruction, &test_accounts.accounts);
        assert!(result.program_result.is_err());
    }

    #[test]
    fn test_verify_non_membership_zero_timestamp() {
        let consensus_state = ConsensusState {
            height: HEIGHT,
            timestamp: 0,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            default_client_state(DEFAULT_CLIENT_ID, HEIGHT),
            consensus_state,
        );

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: vec![],
            path: vec![b"test/path".to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::ConsensusTimestampNotFound).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_happy_path() {
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        let zero_commitment = [0u8; 32];

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, zero_commitment)],
        );

        let signature = attestor.sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![signature],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_non_zero_commitment() {
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        // Non-zero commitment - should fail non-membership check
        let non_zero_commitment = [0xAB; 32];

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, non_zero_commitment)],
        );

        let signature = attestor.sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![signature],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::NonZeroCommitment).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_multi_attestor_quorum() {
        use crate::test_helpers::signing::create_test_attestors;

        let attestors = create_test_attestors(3);
        let attestor_addresses: Vec<[u8; 20]> = attestors.iter().map(|a| a.eth_address).collect();

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses,
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        let zero_commitment = [0u8; 32];

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, zero_commitment)],
        );

        // Sign with 2 out of 3 attestors
        let sig1 = attestors[0].sign(&attestation_data);
        let sig2 = attestors[1].sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![sig1, sig2],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_unknown_signer() {
        let trusted_attestor = TestAttestor::new(1);
        let untrusted_attestor = TestAttestor::new(99);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![trusted_attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        let zero_commitment = [0u8; 32];

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, zero_commitment)],
        );

        // Sign with untrusted attestor
        let signature = untrusted_attestor.sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![signature],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::UnknownSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_path_not_in_attestation() {
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        // Attestation contains a different path
        let attested_path = b"ibc/commitments/channel-0/sequence/1";
        let attested_path_hash = crate::crypto::hash_path(attested_path);

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(attested_path_hash, [0u8; 32])],
        );

        let signature = attestor.sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![signature],
        };

        // Try to verify non-membership for a different path
        let different_path = b"ibc/commitments/channel-0/sequence/999";
        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![different_path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::NotMember).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_attestation_height_mismatch() {
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        let zero_commitment = [0u8; 32];

        // Attestation has different height than consensus state
        let wrong_height = HEIGHT + 10;
        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            wrong_height,
            &[(path_hash, zero_commitment)],
        );

        let signature = attestor.sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![signature],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::HeightMismatch).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_more_signatures_than_required() {
        // 5 attestors, only 3 required, but all 5 sign - should succeed
        let attestors: Vec<_> = (1..=5).map(TestAttestor::new).collect();
        let addresses: Vec<_> = attestors.iter().map(|a| a.eth_address).collect();

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: addresses,
            min_required_sigs: 3,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        let zero_commitment = [0u8; 32];

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, zero_commitment)],
        );

        // All 5 attestors sign
        let signatures: Vec<_> = attestors
            .iter()
            .map(|a| a.sign(&attestation_data))
            .collect();

        let proof = MembershipProof {
            attestation_data,
            signatures,
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_mixed_trusted_and_unknown_signer() {
        // First signer is trusted, second is unknown - should fail
        let trusted_attestor = TestAttestor::new(1);
        let unknown_attestor = TestAttestor::new(2);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![trusted_attestor.eth_address],
            min_required_sigs: 2,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        let zero_commitment = [0u8; 32];

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, zero_commitment)],
        );

        let sig1 = trusted_attestor.sign(&attestation_data);
        let sig2 = unknown_attestor.sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![sig1, sig2],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::err(
            anchor_lang::error::Error::from(ErrorCode::UnknownSigner).into(),
        )];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }

    #[test]
    fn test_verify_non_membership_single_attestor_happy_path() {
        // Minimal quorum: 1 attestor, 1 required signature
        let attestor = TestAttestor::new(1);

        let client_state = ClientState {
            version: crate::types::AccountVersion::V1,
            client_id: DEFAULT_CLIENT_ID.to_string(),
            attestor_addresses: vec![attestor.eth_address],
            min_required_sigs: 1,
            latest_height: HEIGHT,
            is_frozen: false,
        };

        let test_accounts = setup_test_accounts(
            DEFAULT_CLIENT_ID,
            HEIGHT,
            client_state,
            default_consensus_state(HEIGHT),
        );

        let path = b"ibc/commitments/channel-0/sequence/1";
        let path_hash = crate::crypto::hash_path(path);
        let zero_commitment = [0u8; 32];

        let attestation_data = crate::test_helpers::fixtures::encode_packet_attestation(
            HEIGHT,
            &[(path_hash, zero_commitment)],
        );

        let signature = attestor.sign(&attestation_data);

        let proof = MembershipProof {
            attestation_data,
            signatures: vec![signature],
        };

        let msg = NonMembershipMsg {
            height: HEIGHT,
            proof: proof.try_to_vec().unwrap(),
            path: vec![path.to_vec()],
        };

        let instruction = create_verify_non_membership_instruction(&test_accounts, msg);

        let mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        let checks = vec![Check::success()];
        mollusk.process_and_validate_instruction(&instruction, &test_accounts.accounts, &checks);
    }
}
