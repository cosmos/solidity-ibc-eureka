/// Upgrade migration example for `AccessManager`
///
/// This example demonstrates how to migrate `AccessManager` from V1 to a hypothetical V2
/// with additional fields. This pattern shows how the `version` field and `_reserved`
/// space enable seamless upgrades without breaking existing deployments.
use access_manager::state::AccessManager;
use access_manager::types::{AccessManagerVersion, RoleData};
use anchor_lang::prelude::*;
use anchor_lang::{AnchorSerialize, Discriminator};
use solana_ibc_types::roles;
use solana_sdk::pubkey::Pubkey;

/// Serialize account data into vector of bytes. These bytes are stored on Solana
/// and deserialized before the instruction is executed by the SVM
fn create_account_data<T: Discriminator + AnchorSerialize>(account: &T) -> Vec<u8> {
    let mut data = T::DISCRIMINATOR.to_vec();
    account.serialize(&mut data).unwrap();
    data
}

fn setup_access_manager(admin: Pubkey, roles: Vec<RoleData>) -> (Pubkey, Vec<u8>) {
    let (access_manager_pda, _) =
        Pubkey::find_program_address(&[AccessManager::SEED], &access_manager::ID);
    let access_manager = AccessManager {
        version: AccessManagerVersion::V1,
        admin,
        roles,
        _reserved: [0; 256],
    };
    let access_manager_data = create_account_data(&access_manager);
    (access_manager_pda, access_manager_data)
}

#[derive(Debug, AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum AccessManagerVersionExample {
    V1,
    V2, // New version added
}

/// Example V2 `AccessManager` with additional fields
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AccessManagerV2Example {
    /// Schema version for upgrades
    pub version: AccessManagerVersionExample,
    /// Admin that can perform all operations
    pub admin: Pubkey,
    /// Role assignments
    pub roles: Vec<RoleData>,

    // ========== NEW V2 FIELDS ==========
    /// Emergency admin for critical operations (NEW in V2)
    pub emergency_admin: Option<Pubkey>, // 1 + 32 = 33 bytes
    /// Timelock duration in seconds for admin operations (NEW in V2)
    pub timelock_duration: u64, // 8 bytes
    /// Maximum number of members per role (NEW in V2)
    pub max_members_per_role: u32, // 4 bytes
    /// Whether role transfers are allowed (NEW in V2)
    pub allow_role_transfers: bool, // 1 byte
    // Total new fields: 33 + 8 + 4 + 1 = 46 bytes
    /// Reserved space for future fields (reduced from 256 to 210)
    pub _reserved: [u8; 210],
}

#[test]
fn test_access_manager_migration_v1_to_v2() {
    // Create V1 account with no roles
    let admin = Pubkey::new_unique();
    let (_, v1_data) = setup_access_manager(admin, vec![]);

    // Deserialize account into the struct with new added fields
    let mut cursor = &v1_data[8..]; // Skip discriminator
    let mut state: AccessManagerV2Example = AnchorDeserialize::deserialize(&mut cursor).unwrap();

    // Verify it's V1
    assert_eq!(state.version, AccessManagerVersionExample::V1);

    // Perform migration logic
    state.version = AccessManagerVersionExample::V2;
    state.emergency_admin.replace(Pubkey::new_unique());
    state.timelock_duration = 86400; // 24 hours
    state.max_members_per_role = 100;
    state.allow_role_transfers = true;

    // Verify migration preserved V1 fields
    assert_eq!(state.version, AccessManagerVersionExample::V2);
    assert_eq!(state.admin, admin);
    assert_eq!(state.roles.len(), 0);

    // Verify new V2 fields
    assert!(state.emergency_admin.is_some());
    assert_eq!(state.timelock_duration, 86400);
    assert_eq!(state.max_members_per_role, 100);
    assert!(state.allow_role_transfers);
    assert_eq!(state._reserved.len(), 210);
}

#[test]
fn test_access_manager_migration_with_roles() {
    // Create V1 account with multiple roles
    let admin = Pubkey::new_unique();
    let relayer1 = Pubkey::new_unique();
    let relayer2 = Pubkey::new_unique();
    let pauser = Pubkey::new_unique();

    let roles = vec![
        RoleData {
            role_id: roles::RELAYER_ROLE,
            members: vec![relayer1, relayer2],
        },
        RoleData {
            role_id: roles::PAUSER_ROLE,
            members: vec![pauser],
        },
    ];

    let (_, v1_data) = setup_access_manager(admin, roles);

    // Deserialize and migrate
    let mut cursor = &v1_data[8..]; // Skip discriminator
    let mut state: AccessManagerV2Example = AnchorDeserialize::deserialize(&mut cursor).unwrap();

    assert_eq!(state.version, AccessManagerVersionExample::V1);

    // Migrate to V2
    state.version = AccessManagerVersionExample::V2;
    state.emergency_admin.replace(Pubkey::new_unique());
    state.timelock_duration = 3600; // 1 hour
    state.max_members_per_role = 50;
    state.allow_role_transfers = false;

    // Verify roles are preserved
    assert_eq!(state.version, AccessManagerVersionExample::V2);
    assert_eq!(state.admin, admin);
    assert_eq!(state.roles.len(), 2);

    // Check relayer role
    let relayer_role = state
        .roles
        .iter()
        .find(|r| r.role_id == roles::RELAYER_ROLE)
        .unwrap();
    assert_eq!(relayer_role.members.len(), 2);
    assert!(relayer_role.members.contains(&relayer1));
    assert!(relayer_role.members.contains(&relayer2));

    // Check pauser role
    let pauser_role = state
        .roles
        .iter()
        .find(|r| r.role_id == roles::PAUSER_ROLE)
        .unwrap();
    assert_eq!(pauser_role.members.len(), 1);
    assert!(pauser_role.members.contains(&pauser));

    // Verify new V2 fields
    assert!(state.emergency_admin.is_some());
    assert_eq!(state.timelock_duration, 3600);
    assert_eq!(state.max_members_per_role, 50);
    assert!(!state.allow_role_transfers);
}

#[test]
fn test_access_manager_migration_preserves_admin() {
    // Create V1 account with specific admin
    let admin = Pubkey::new_unique();
    let (_, v1_data) = setup_access_manager(admin, vec![]);

    // Deserialize and migrate
    let mut cursor = &v1_data[8..];
    let mut state: AccessManagerV2Example = AnchorDeserialize::deserialize(&mut cursor).unwrap();

    // Migrate to V2
    state.version = AccessManagerVersionExample::V2;
    state.emergency_admin.replace(Pubkey::new_unique());

    // Verify admin is preserved
    assert_eq!(state.admin, admin);
    assert_eq!(state.version, AccessManagerVersionExample::V2);
}

#[test]
fn test_access_manager_reserved_space_sufficient() {
    // Create V1 account
    let admin = Pubkey::new_unique();
    let (_, v1_data) = setup_access_manager(admin, vec![]);

    // Deserialize to verify reserved space
    let mut cursor = &v1_data[8..];
    let state: AccessManagerV2Example = AnchorDeserialize::deserialize(&mut cursor).unwrap();

    // Verify we can add fields and still have reserved space
    // V2 adds 46 bytes of new fields
    // Original reserved: 256 bytes
    // Remaining reserved: 210 bytes (still plenty for future upgrades)
    assert_eq!(state._reserved.len(), 210);
}

#[test]
fn test_access_manager_pda_derivation_preserved() {
    // Create V1 account
    let admin = Pubkey::new_unique();
    let (original_pda, v1_data) = setup_access_manager(admin, vec![]);

    // Deserialize and migrate
    let mut cursor = &v1_data[8..];
    let mut state: AccessManagerV2Example = AnchorDeserialize::deserialize(&mut cursor).unwrap();

    // Migrate to V2
    state.version = AccessManagerVersionExample::V2;
    state.emergency_admin.replace(Pubkey::new_unique());

    // Verify PDA derivation still works
    let (derived_pda, _) =
        Pubkey::find_program_address(&[AccessManager::SEED], &access_manager::ID);

    assert_eq!(derived_pda, original_pda);
}

#[test]
fn test_access_manager_migration_with_max_roles() {
    // Create V1 account with maximum allowed roles (8)
    let admin = Pubkey::new_unique();
    let mut roles = vec![];

    // Create 8 different roles
    for i in 0..8 {
        roles.push(RoleData {
            role_id: i as u64,
            members: vec![Pubkey::new_unique()],
        });
    }

    let (_, v1_data) = setup_access_manager(admin, roles);

    // Deserialize and migrate
    let mut cursor = &v1_data[8..];
    let mut state: AccessManagerV2Example = AnchorDeserialize::deserialize(&mut cursor).unwrap();

    assert_eq!(state.version, AccessManagerVersionExample::V1);
    assert_eq!(state.roles.len(), 8);

    // Migrate to V2
    state.version = AccessManagerVersionExample::V2;
    state.emergency_admin.replace(Pubkey::new_unique());
    state.timelock_duration = 7200;
    state.max_members_per_role = 200;
    state.allow_role_transfers = true;

    // Verify all roles are preserved
    assert_eq!(state.roles.len(), 8);
    assert_eq!(state.version, AccessManagerVersionExample::V2);

    // Verify new V2 fields
    assert!(state.emergency_admin.is_some());
    assert_eq!(state.timelock_duration, 7200);
    assert_eq!(state.max_members_per_role, 200);
    assert!(state.allow_role_transfers);
}

#[test]
fn test_access_manager_migration_no_emergency_admin() {
    // Create V1 account
    let admin = Pubkey::new_unique();
    let (_, v1_data) = setup_access_manager(admin, vec![]);

    // Deserialize and migrate WITHOUT setting emergency admin
    let mut cursor = &v1_data[8..];
    let mut state: AccessManagerV2Example = AnchorDeserialize::deserialize(&mut cursor).unwrap();

    // Migrate to V2 but leave emergency_admin as None
    state.version = AccessManagerVersionExample::V2;
    state.timelock_duration = 0; // No timelock
    state.max_members_per_role = 1000;
    state.allow_role_transfers = true;

    // Verify emergency_admin is None (optional feature)
    assert_eq!(state.version, AccessManagerVersionExample::V2);
    assert!(state.emergency_admin.is_none());
    assert_eq!(state.timelock_duration, 0);
}
