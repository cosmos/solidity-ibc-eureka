use crate::state::{AccountVersion, RouterState};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(
        init,
        payer = payer,
        space = 8 + RouterState::INIT_SPACE,
        seeds = [RouterState::SEED],
        bump
    )]
    pub router_state: Account<'info, RouterState>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn initialize(ctx: Context<Initialize>, authority: Pubkey) -> Result<()> {
    let router_state = &mut ctx.accounts.router_state;
    router_state.version = AccountVersion::V1;
    router_state.authority = authority;
    router_state._reserved = [0u8; 256];
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    #[test]
    fn test_initialize_happy_path() {
        let payer = Pubkey::new_unique();
        let authority = Pubkey::new_unique();

        let (router_state_pda, _) = Pubkey::find_program_address(&[RouterState::SEED], &crate::ID);

        let instruction_data = crate::instruction::Initialize { authority };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(router_state_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (
                router_state_pda,
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

        let mollusk = Mollusk::new(&crate::ID, crate::get_router_program_path());

        let checks = vec![
            Check::success(),
            Check::account(&router_state_pda).owner(&crate::ID).build(),
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

        let router_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &router_state_pda)
            .map(|(_, account)| account)
            .expect("Router state account not found");

        assert!(
            router_state_account.lamports > 0,
            "Router state account should be rent-exempt"
        );
        assert!(
            router_state_account.data.len() > 8,
            "Router state account should have data"
        );

        let deserialized_router_state: RouterState =
            RouterState::try_deserialize(&mut &router_state_account.data[..])
                .expect("Failed to deserialize router state");

        assert_eq!(deserialized_router_state.authority, authority);
    }
}
