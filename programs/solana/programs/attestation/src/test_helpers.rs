pub const PROGRAM_BINARY_PATH: &str = "../../target/deploy/attestation";

pub mod accounts {
    use crate::state::ConsensusStateStore;
    use crate::types::{AppState, ClientState};
    use anchor_lang::AccountSerialize;
    use solana_sdk::account::Account;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    pub fn create_client_state_account(client_state: &ClientState) -> Account {
        let mut data = vec![];
        client_state.try_serialize(&mut data).unwrap();
        Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_consensus_state_account(height: u64, timestamp: u64) -> Account {
        let consensus_state_store = ConsensusStateStore { height, timestamp };
        let mut data = vec![];
        consensus_state_store.try_serialize(&mut data).unwrap();
        Account {
            lamports: 10_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: u64::MAX, // Required for Mollusk rent exemption checks
        }
    }

    pub fn create_app_state_account(access_manager: Pubkey) -> Account {
        let app_state = AppState {
            version: crate::types::AccountVersion::V1,
            access_manager,
            _reserved: [0; 256],
        };
        let mut data = vec![];
        app_state.try_serialize(&mut data).unwrap();
        Account {
            lamports: 1_000_000,
            data,
            owner: crate::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_payer_account() -> Account {
        Account {
            lamports: 10_000_000_000,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_empty_account() -> Account {
        Account {
            lamports: 0,
            data: vec![],
            owner: system_program::ID,
            executable: false,
            rent_epoch: 0,
        }
    }

    pub fn create_system_program_account() -> Account {
        Account {
            lamports: 0,
            data: vec![],
            owner: native_loader::ID,
            executable: true,
            rent_epoch: 0,
        }
    }

    pub fn create_instructions_sysvar_account() -> (Pubkey, Account) {
        use solana_sdk::sysvar::instructions::{
            construct_instructions_data, BorrowedAccountMeta, BorrowedInstruction,
        };

        let account_pubkey = Pubkey::new_unique();
        let account = BorrowedAccountMeta {
            pubkey: &account_pubkey,
            is_signer: false,
            is_writable: true,
        };
        let mock_instruction = BorrowedInstruction {
            program_id: &crate::ID,
            accounts: vec![account],
            data: &[],
        };

        let ixs_data = construct_instructions_data(&[mock_instruction]);

        (
            solana_sdk::sysvar::instructions::ID,
            Account {
                lamports: 1_000_000,
                data: ixs_data,
                owner: solana_sdk::sysvar::ID,
                executable: false,
                rent_epoch: 0,
            },
        )
    }
}

pub mod fixtures {
    use crate::types::{ClientState, PacketAttestation, PacketCompact, StateAttestation};
    use crate::ETH_ADDRESS_LEN;
    use alloy_sol_types::SolValue;

    pub const DEFAULT_TIMESTAMP: u64 = 1_700_000_000;

    pub fn default_client_state(height: u64) -> ClientState {
        ClientState {
            version: crate::types::AccountVersion::V1,
            attestor_addresses: vec![[1u8; 20]],
            min_required_sigs: 1,
            latest_height: height,
            is_frozen: false,
        }
    }

    pub fn create_test_client_state(
        attestor_addresses: Vec<[u8; ETH_ADDRESS_LEN]>,
        min_required_sigs: u8,
        latest_height: u64,
    ) -> ClientState {
        ClientState {
            version: crate::types::AccountVersion::V1,
            attestor_addresses,
            min_required_sigs,
            latest_height,
            is_frozen: false,
        }
    }

    pub fn encode_packet_attestation(height: u64, packets: &[([u8; 32], [u8; 32])]) -> Vec<u8> {
        PacketAttestation {
            height,
            packets: packets
                .iter()
                .map(|(path, commitment)| PacketCompact {
                    path: (*path).into(),
                    commitment: (*commitment).into(),
                })
                .collect(),
        }
        .abi_encode()
    }

    pub fn encode_state_attestation(height: u64, timestamp: u64) -> Vec<u8> {
        StateAttestation { height, timestamp }.abi_encode()
    }

    pub fn create_test_signature() -> Vec<u8> {
        let mut sig = vec![0u8; 65];
        sig[64] = 27; // recovery_id
        sig
    }
}

/// Test signing utilities using alloy.
pub mod signing {
    use crate::ETH_ADDRESS_LEN;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
    use sha2::{Digest, Sha256};

    /// Test attestor with deterministic keys for unit and integration testing.
    pub struct TestAttestor {
        signer: PrivateKeySigner,
        pub eth_address: [u8; ETH_ADDRESS_LEN],
    }

    impl TestAttestor {
        /// Create a test attestor from a deterministic seed
        pub fn new(seed: u8) -> Self {
            let mut key_bytes = [0u8; 32];
            key_bytes[0] = seed;
            key_bytes[31] = 1; // Ensure non-zero

            let signer = PrivateKeySigner::from_bytes(&key_bytes.into())
                .expect("Valid key bytes for testing");
            let eth_address = signer.address().0 .0;

            Self {
                signer,
                eth_address,
            }
        }

        /// Sign attestation data and return 65-byte signature with Ethereum-style recovery id
        pub fn sign(&self, data: &[u8]) -> Vec<u8> {
            let message_hash: [u8; 32] = Sha256::digest(data).into();
            let sig = self
                .signer
                .sign_hash_sync(&message_hash.into())
                .expect("Signing should succeed");

            let mut result = Vec::with_capacity(65);
            result.extend_from_slice(&sig.r().to_be_bytes::<32>());
            result.extend_from_slice(&sig.s().to_be_bytes::<32>());
            result.push(u8::from(sig.v()) + 27);
            result
        }
    }

    /// Create a set of test attestors
    pub fn create_test_attestors(count: usize) -> Vec<TestAttestor> {
        (1..=count as u8).map(TestAttestor::new).collect()
    }
}

pub mod access_control {
    use access_manager::RoleData;
    use anchor_lang::prelude::Pubkey;
    use anchor_lang::{AnchorSerialize, Discriminator};

    pub fn setup_access_manager(admin: Pubkey, relayers: Vec<Pubkey>) -> (Pubkey, Vec<u8>) {
        let (access_manager_pda, _) = Pubkey::find_program_address(
            &[access_manager::state::AccessManager::SEED],
            &access_manager::ID,
        );

        let mut roles = vec![RoleData {
            role_id: solana_ibc_types::roles::ADMIN_ROLE,
            members: vec![admin],
        }];

        if !relayers.is_empty() {
            roles.push(RoleData {
                role_id: solana_ibc_types::roles::RELAYER_ROLE,
                members: relayers,
            });
        }

        let access_manager = access_manager::state::AccessManager { roles };

        let mut data = access_manager::state::AccessManager::DISCRIMINATOR.to_vec();
        access_manager.serialize(&mut data).unwrap();

        (access_manager_pda, data)
    }

    pub fn create_access_manager_account(
        admin: Pubkey,
        relayers: Vec<Pubkey>,
    ) -> (Pubkey, solana_sdk::account::Account) {
        let (pda, data) = setup_access_manager(admin, relayers);

        let account = solana_sdk::account::Account {
            lamports: 10_000_000,
            data,
            owner: access_manager::ID,
            executable: false,
            rent_epoch: 0,
        };

        (pda, account)
    }
}
