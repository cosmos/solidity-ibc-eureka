use crate::types::{AccessManagerVersion, RoleData};
use anchor_lang::prelude::*;
use solana_ibc_types::roles;

#[account]
#[derive(InitSpace, Debug)]
pub struct AccessManager {
    pub version: AccessManagerVersion,
    pub admin: Pubkey,
    #[max_len(8)]
    pub roles: Vec<RoleData>,
    pub _reserved: [u8; 256],
}

impl AccessManager {
    pub const SEED: &'static [u8] = b"access_manager";

    pub fn has_role(&self, role_id: u64, account: &Pubkey) -> bool {
        if role_id != roles::PUBLIC_ROLE && self.admin == *account {
            return true;
        }

        if role_id == roles::PUBLIC_ROLE {
            return true;
        }

        self.roles
            .iter()
            .find(|r| r.role_id == role_id)
            .is_some_and(|r| r.members.contains(account))
    }

    pub fn grant_role(&mut self, role_id: u64, account: Pubkey) -> Result<()> {
        if let Some(role) = self.roles.iter_mut().find(|r| r.role_id == role_id) {
            if !role.members.contains(&account) {
                role.members.push(account);
            }
        } else {
            self.roles.push(RoleData {
                role_id,
                members: vec![account],
            });
        }
        Ok(())
    }

    pub fn revoke_role(&mut self, role_id: u64, account: &Pubkey) -> Result<()> {
        if let Some(role) = self.roles.iter_mut().find(|r| r.role_id == role_id) {
            role.members.retain(|m| m != account);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_access_manager(admin: Pubkey) -> AccessManager {
        AccessManager {
            version: AccessManagerVersion::V1,
            admin,
            roles: vec![],
            _reserved: [0; 256],
        }
    }

    fn create_access_manager_with_roles(admin: Pubkey, roles: Vec<RoleData>) -> AccessManager {
        AccessManager {
            version: AccessManagerVersion::V1,
            admin,
            roles,
            _reserved: [0; 256],
        }
    }

    #[test]
    fn test_admin_has_all_roles() {
        let admin = Pubkey::new_unique();
        let access_manager = create_access_manager(admin);

        assert!(access_manager.has_role(roles::RELAYER_ROLE, &admin));
        assert!(access_manager.has_role(roles::PAUSER_ROLE, &admin));
        assert!(access_manager.has_role(roles::UNPAUSER_ROLE, &admin));
    }

    #[test]
    fn test_public_role_accessible_to_all() {
        let admin = Pubkey::new_unique();
        let anyone = Pubkey::new_unique();
        let access_manager = create_access_manager(admin);

        assert!(access_manager.has_role(roles::PUBLIC_ROLE, &anyone));
        assert!(access_manager.has_role(roles::PUBLIC_ROLE, &admin));
    }

    #[test]
    fn test_grant_role() {
        let admin = Pubkey::new_unique();
        let relayer = Pubkey::new_unique();
        let mut access_manager = create_access_manager(admin);

        assert!(!access_manager.has_role(roles::RELAYER_ROLE, &relayer));

        access_manager
            .grant_role(roles::RELAYER_ROLE, relayer)
            .unwrap();

        assert!(access_manager.has_role(roles::RELAYER_ROLE, &relayer));
    }

    #[test]
    fn test_revoke_role() {
        let admin = Pubkey::new_unique();
        let relayer = Pubkey::new_unique();
        let mut access_manager = create_access_manager_with_roles(
            admin,
            vec![RoleData {
                role_id: roles::RELAYER_ROLE,
                members: vec![relayer],
            }],
        );

        assert!(access_manager.has_role(roles::RELAYER_ROLE, &relayer));

        access_manager
            .revoke_role(roles::RELAYER_ROLE, &relayer)
            .unwrap();

        assert!(!access_manager.has_role(roles::RELAYER_ROLE, &relayer));
    }

    #[test]
    fn test_grant_role_idempotent() {
        let admin = Pubkey::new_unique();
        let relayer = Pubkey::new_unique();
        let mut access_manager = create_access_manager(admin);

        access_manager
            .grant_role(roles::RELAYER_ROLE, relayer)
            .unwrap();
        access_manager
            .grant_role(roles::RELAYER_ROLE, relayer)
            .unwrap();

        let role = access_manager
            .roles
            .iter()
            .find(|r| r.role_id == roles::RELAYER_ROLE)
            .unwrap();
        assert_eq!(role.members.len(), 1);
    }

    #[test]
    fn test_multiple_members_per_role() {
        let admin = Pubkey::new_unique();
        let relayer1 = Pubkey::new_unique();
        let relayer2 = Pubkey::new_unique();
        let mut access_manager = create_access_manager(admin);

        access_manager
            .grant_role(roles::RELAYER_ROLE, relayer1)
            .unwrap();
        access_manager
            .grant_role(roles::RELAYER_ROLE, relayer2)
            .unwrap();

        assert!(access_manager.has_role(roles::RELAYER_ROLE, &relayer1));
        assert!(access_manager.has_role(roles::RELAYER_ROLE, &relayer2));
    }
}
