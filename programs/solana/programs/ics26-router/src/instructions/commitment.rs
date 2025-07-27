use crate::errors::RouterError;
use crate::state::{Commitment, RouterState, COMMITMENT_SEED, ROUTER_STATE_SEED};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(path_hash: [u8; 32])]
pub struct StoreCommitment<'info> {
    #[account(
        seeds = [ROUTER_STATE_SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(
        init,
        payer = payer,
        space = 8 + Commitment::INIT_SPACE,
        seeds = [COMMITMENT_SEED, &path_hash],
        bump
    )]
    pub commitment: Account<'info, Commitment>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(path_hash: [u8; 32])]
pub struct GetCommitment<'info> {
    #[account(
        seeds = [COMMITMENT_SEED, &path_hash],
        bump
    )]
    pub commitment: Account<'info, Commitment>,
}

pub fn store_commitment(
    ctx: Context<StoreCommitment>,
    _path_hash: [u8; 32],
    commitment: [u8; 32],
) -> Result<()> {
    let router_state = &ctx.accounts.router_state;

    require!(
        ctx.accounts.authority.key() == router_state.authority,
        RouterError::UnauthorizedSender
    );

    let commitment_account = &mut ctx.accounts.commitment;
    commitment_account.value = commitment;

    Ok(())
}

pub fn get_commitment(ctx: Context<GetCommitment>, _path_hash: [u8; 32]) -> Result<[u8; 32]> {
    Ok(ctx.accounts.commitment.value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::RouterState;
    use anchor_lang::{AnchorDeserialize, InstructionData};
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::program_error::ProgramError;
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    fn create_account_data(
        account_name: &str,
        init_space: usize,
        serialize_fn: impl FnOnce(&mut [u8]),
    ) -> Vec<u8> {
        let mut data = vec![0u8; 8 + init_space];

        let discriminator: [u8; 8] =
            anchor_lang::solana_program::hash::hash(format!("account:{account_name}").as_bytes())
                .to_bytes()[..8]
                .try_into()
                .unwrap();
        data[0..8].copy_from_slice(&discriminator);

        serialize_fn(&mut data[8..]);

        data
    }

    fn setup_router_state(authority: Pubkey) -> (Pubkey, Vec<u8>) {
        let (router_state_pda, _) = Pubkey::find_program_address(&[ROUTER_STATE_SEED], &crate::ID);

        let router_state_data =
            create_account_data("RouterState", RouterState::INIT_SPACE, |data| {
                data[0..32].copy_from_slice(authority.as_ref()); // authority: Pubkey
                data[32] = 1; // initialized: bool = true
            });

        (router_state_pda, router_state_data)
    }

    #[test]
    fn test_store_commitment_happy_path() {
        let authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let path_hash: [u8; 32] = [1u8; 32];
        let commitment_value: [u8; 32] = [2u8; 32];

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (commitment_pda, _) =
            Pubkey::find_program_address(&[COMMITMENT_SEED, &path_hash], &crate::ID);

        let instruction_data = crate::instruction::StoreCommitment {
            path_hash,
            commitment: commitment_value,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(commitment_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(authority, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (
                router_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: router_state_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                commitment_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payer,
                Account {
                    lamports: payer_lamports,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                authority,
                Account {
                    lamports: 1_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                system_program::ID,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::ROUTER_PROGRAM_PATH);

        let checks = vec![
            Check::success(),
            Check::account(&commitment_pda).owner(&crate::ID).build(),
        ];

        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);

        let payer_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &payer)
            .map(|(_, account)| account)
            .expect("Payer account not found");

        assert!(
            payer_account.lamports < payer_lamports,
            "Payer should have paid for account creation"
        );

        let commitment_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &commitment_pda)
            .map(|(_, account)| account)
            .expect("Commitment account not found");

        assert!(
            commitment_account.lamports > 0,
            "Commitment account should be rent-exempt"
        );

        let mut data_slice = &commitment_account.data[8..];
        let deserialized_commitment: Commitment =
            Commitment::deserialize(&mut data_slice).expect("Failed to deserialize commitment");

        assert_eq!(deserialized_commitment.value, commitment_value);
    }

    #[test]
    fn test_get_commitment_happy_path() {
        let path_hash: [u8; 32] = [1u8; 32];
        let commitment_value: [u8; 32] = [2u8; 32];

        let (commitment_pda, _) =
            Pubkey::find_program_address(&[COMMITMENT_SEED, &path_hash], &crate::ID);

        let commitment_data = create_account_data("Commitment", Commitment::INIT_SPACE, |data| {
            data[0..32].copy_from_slice(&commitment_value);
        });

        let instruction_data = crate::instruction::GetCommitment { path_hash };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![AccountMeta::new_readonly(commitment_pda, false)],
            data: instruction_data.data(),
        };

        let accounts = vec![(
            commitment_pda,
            Account {
                lamports: 1_000_000,
                data: commitment_data,
                owner: crate::ID,
                executable: false,
                rent_epoch: 0,
            },
        )];

        let mollusk = Mollusk::new(&crate::ID, crate::ROUTER_PROGRAM_PATH);

        let checks = vec![Check::success(), Check::return_data(&commitment_value)];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }

    #[test]
    fn test_store_commitment_unauthorized() {
        let authority = Pubkey::new_unique();
        let wrong_authority = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let path_hash: [u8; 32] = [1u8; 32];
        let commitment_value: [u8; 32] = [2u8; 32];

        let (router_state_pda, router_state_data) = setup_router_state(authority);
        let (commitment_pda, _) =
            Pubkey::find_program_address(&[COMMITMENT_SEED, &path_hash], &crate::ID);

        let instruction_data = crate::instruction::StoreCommitment {
            path_hash,
            commitment: commitment_value,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(router_state_pda, false),
                AccountMeta::new(commitment_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(wrong_authority, true), // Wrong authority
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let accounts = vec![
            (
                router_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: router_state_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                commitment_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                payer,
                Account {
                    lamports: 10_000_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                wrong_authority,
                Account {
                    lamports: 1_000_000,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                system_program::ID,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: native_loader::ID,
                    executable: true,
                    rent_epoch: 0,
                },
            ),
        ];

        let mollusk = Mollusk::new(&crate::ID, crate::ROUTER_PROGRAM_PATH);

        let checks = vec![
            Check::err(ProgramError::Custom(6000)), // RouterError::UnauthorizedSender
        ];

        mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
    }
}
