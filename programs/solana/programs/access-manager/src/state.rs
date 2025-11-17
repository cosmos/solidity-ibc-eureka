use anchor_lang::prelude::*;
use derive_more::{Deref, DerefMut};
use solana_ibc_types::access_manager::RoleData;
use solana_ibc_types::roles;

/// Access manager account - wraps the shared type from solana-ibc-types
#[account]
#[derive(InitSpace, Debug, Deref, DerefMut)]
pub struct AccessManager(pub solana_ibc_types::AccessManager);

impl AccessManager {
    pub const SEED: &'static [u8] = solana_ibc_types::AccessManager::SEED;
    pub const UPGRADE_AUTHORITY_SEED: &'static [u8] = b"upgrade_authority";

    /// Get upgrade authority PDA for a target program
    pub fn upgrade_authority_pda(target_program: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[Self::UPGRADE_AUTHORITY_SEED, target_program.as_ref()],
            program_id,
        )
    }

    pub fn has_role(&self, role_id: u64, account: &Pubkey) -> bool {
        // PUBLIC_ROLE is accessible to everyone
        if role_id == roles::PUBLIC_ROLE {
            return true;
        }

        // Check if account has the specific role
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
        // Prevent removing the last admin
        if role_id == roles::ADMIN_ROLE && self.is_last_admin(account) {
            return Err(crate::errors::AccessManagerError::CannotRemoveLastAdmin.into());
        }

        if let Some(role) = self.roles.iter_mut().find(|r| r.role_id == role_id) {
            role.members.retain(|m| m != account);
        }
        Ok(())
    }

    /// Check if an account is the last admin
    fn is_last_admin(&self, account: &Pubkey) -> bool {
        self.roles
            .iter()
            .find(|r| r.role_id == roles::ADMIN_ROLE)
            .is_some_and(|admin_role| {
                admin_role.members.len() == 1 && admin_role.members.contains(account)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_access_manager() -> AccessManager {
        AccessManager(solana_ibc_types::AccessManager { roles: vec![] })
    }

    fn create_access_manager_with_roles(roles: Vec<solana_ibc_types::RoleData>) -> AccessManager {
        AccessManager(solana_ibc_types::AccessManager { roles })
    }

    #[test]
    fn test_admin_does_not_auto_pass_roles() {
        let admin = Pubkey::new_unique();
        let mut access_manager = create_access_manager();

        // Grant admin role
        access_manager.grant_role(roles::ADMIN_ROLE, admin).unwrap();

        // Admin should have ADMIN_ROLE
        assert!(access_manager.has_role(roles::ADMIN_ROLE, &admin));

        // Admin should NOT automatically have other roles
        assert!(!access_manager.has_role(roles::RELAYER_ROLE, &admin));
        assert!(!access_manager.has_role(roles::PAUSER_ROLE, &admin));
        assert!(!access_manager.has_role(roles::UNPAUSER_ROLE, &admin));
    }

    #[test]
    fn test_public_role_accessible_to_all() {
        let anyone = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let access_manager = create_access_manager();

        assert!(access_manager.has_role(roles::PUBLIC_ROLE, &anyone));
        assert!(access_manager.has_role(roles::PUBLIC_ROLE, &admin));
    }

    #[test]
    fn test_grant_role() {
        let relayer = Pubkey::new_unique();
        let mut access_manager = create_access_manager();

        assert!(!access_manager.has_role(roles::RELAYER_ROLE, &relayer));

        access_manager
            .grant_role(roles::RELAYER_ROLE, relayer)
            .unwrap();

        assert!(access_manager.has_role(roles::RELAYER_ROLE, &relayer));
    }

    #[test]
    fn test_revoke_role() {
        let relayer = Pubkey::new_unique();
        let mut access_manager =
            create_access_manager_with_roles(vec![solana_ibc_types::RoleData {
                role_id: roles::RELAYER_ROLE,
                members: vec![relayer],
            }]);

        assert!(access_manager.has_role(roles::RELAYER_ROLE, &relayer));

        access_manager
            .revoke_role(roles::RELAYER_ROLE, &relayer)
            .unwrap();

        assert!(!access_manager.has_role(roles::RELAYER_ROLE, &relayer));
    }

    #[test]
    fn test_cannot_remove_last_admin() {
        let admin = Pubkey::new_unique();
        let mut access_manager =
            create_access_manager_with_roles(vec![solana_ibc_types::RoleData {
                role_id: roles::ADMIN_ROLE,
                members: vec![admin],
            }]);

        assert!(access_manager.has_role(roles::ADMIN_ROLE, &admin));

        // Should fail to revoke last admin
        let result = access_manager.revoke_role(roles::ADMIN_ROLE, &admin);
        assert!(result.is_err());

        // Admin should still have the role
        assert!(access_manager.has_role(roles::ADMIN_ROLE, &admin));
    }

    #[test]
    fn test_can_remove_non_last_admin() {
        let admin1 = Pubkey::new_unique();
        let admin2 = Pubkey::new_unique();
        let mut access_manager =
            create_access_manager_with_roles(vec![solana_ibc_types::RoleData {
                role_id: roles::ADMIN_ROLE,
                members: vec![admin1, admin2],
            }]);

        // Should succeed to revoke one admin when multiple exist
        access_manager
            .revoke_role(roles::ADMIN_ROLE, &admin1)
            .unwrap();

        assert!(!access_manager.has_role(roles::ADMIN_ROLE, &admin1));
        assert!(access_manager.has_role(roles::ADMIN_ROLE, &admin2));
    }

    #[test]
    fn test_grant_role_idempotent() {
        let relayer = Pubkey::new_unique();
        let mut access_manager = create_access_manager();

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
        let relayer1 = Pubkey::new_unique();
        let relayer2 = Pubkey::new_unique();
        let mut access_manager = create_access_manager();

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
