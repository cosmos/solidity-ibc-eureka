use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTMintReceived;
use crate::helpers::{check_and_update_mint_rate_limit, mint_to_account};
use crate::state::{IFTAppMintState, IFTAppState, IFTBridge, IFTMintMsg};

/// IFT Mint instruction - called by GMP via CPI when receiving a cross-chain mint request.
#[derive(Accounts)]
#[instruction(msg: IFTMintMsg)]
pub struct IFTMint<'info> {
    /// Global IFT app state (read-only, for `gmp_program` and paused check)
    #[account(
        seeds = [IFT_APP_STATE_SEED],
        bump = app_state.bump,
        constraint = !app_state.paused @ IFTError::AppPaused,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// Per-mint IFT app state (mut, for rate limits)
    #[account(
        mut,
        seeds = [IFT_APP_MINT_STATE_SEED, mint.key().as_ref()],
        bump = app_mint_state.bump
    )]
    pub app_mint_state: Account<'info, IFTAppMintState>,

    /// IFT bridge - provides counterparty info for GMP account validation.
    /// Relayer passes the correct bridge; validation ensures bridge matches GMP account.
    /// Security: Anchor verifies ownership, `validate_gmp_account` verifies (`client_id`, counterparty) match.
    #[account(
        constraint = ift_bridge.mint == app_mint_state.mint @ IFTError::InvalidBridge,
        constraint = ift_bridge.active @ IFTError::BridgeNotActive
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// SPL Token mint
    #[account(
        mut,
        address = app_mint_state.mint
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA that signs for minting
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        // Use stored bump to avoid expensive `find_program_address` computation.
        // The bump is needed for PDA signing during the mint CPI call.
        bump = app_mint_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// Receiver's token account (will be created if needed)
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = receiver_owner,
        associated_token::token_program = token_program,
    )]
    pub receiver_token_account: InterfaceAccount<'info, TokenAccount>,

    /// CHECK: Receiver who will own the minted tokens.
    /// Constraint prevents relayer from substituting a different receiver than specified in cross-chain message.
    #[account(
        constraint = receiver_owner.key() == msg.receiver @ IFTError::InvalidReceiver
    )]
    pub receiver_owner: AccountInfo<'info>,

    /// GMP account PDA - validated to match counterparty bridge
    pub gmp_account: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn ift_mint(ctx: Context<IFTMint>, msg: IFTMintMsg) -> Result<()> {
    let clock = Clock::get()?;
    let bridge = &ctx.accounts.ift_bridge;

    require!(msg.amount > 0, IFTError::ZeroAmount);

    // Validate GMP account matches the bridge's (client_id, counterparty_ift_address).
    // This ensures the relayer passed the correct bridge for this GMP call.
    validate_gmp_account(
        &ctx.accounts.gmp_account.key(),
        &bridge.client_id,
        &bridge.counterparty_ift_address,
        &ctx.accounts.app_state.gmp_program,
    )?;

    check_and_update_mint_rate_limit(&mut ctx.accounts.app_mint_state, msg.amount, &clock)?;

    mint_to_account(
        &ctx.accounts.mint,
        &ctx.accounts.receiver_token_account,
        &ctx.accounts.mint_authority,
        ctx.accounts.app_mint_state.mint_authority_bump,
        &ctx.accounts.token_program,
        msg.amount,
    )?;
    ctx.accounts.mint.reload()?;
    ctx.accounts.receiver_token_account.reload()?;

    emit!(IFTMintReceived {
        mint: ctx.accounts.mint.key(),
        client_id: bridge.client_id.clone(),
        receiver: msg.receiver,
        amount: msg.amount,
        gmp_account: ctx.accounts.gmp_account.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

fn validate_gmp_account(
    gmp_account: &Pubkey,
    client_id: &str,
    counterparty_address: &str,
    gmp_program: &Pubkey,
) -> Result<()> {
    use solana_ibc_types::ics27::{GMPAccount, Salt};

    let gmp = GMPAccount::new(
        client_id
            .to_string()
            .try_into()
            .map_err(|_| IFTError::InvalidGmpAccount)?,
        counterparty_address
            .to_string()
            .try_into()
            .map_err(|_| IFTError::InvalidGmpAccount)?,
        Salt::empty(),
        gmp_program,
    );

    require!(gmp.pda == *gmp_account, IFTError::InvalidGmpAccount);
    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::InstructionData;
    use rstest::rstest;
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
    };

    use crate::state::{ChainOptions, IFTMintMsg};
    use crate::test_utils::*;

    const TEST_CLIENT_ID: &str = "07-tendermint-0";
    const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";

    #[derive(Clone, Copy)]
    enum MintErrorCase {
        ZeroAmount,
        ReceiverMismatch,
        GmpNotSigner,
        BridgeNotActive,
        InvalidGmpAccount,
        TokenPaused,
        MintRateLimitExceeded,
    }

    #[allow(clippy::struct_excessive_bools)]
    struct MintTestConfig {
        amount: u64,
        use_wrong_receiver: bool,
        gmp_is_signer: bool,
        bridge_active: bool,
        use_wrong_gmp_account: bool,
        token_paused: bool,
        rate_limit_exceeded: bool,
    }

    impl From<MintErrorCase> for MintTestConfig {
        fn from(case: MintErrorCase) -> Self {
            let default = Self {
                amount: 1000,
                use_wrong_receiver: false,
                gmp_is_signer: true,
                bridge_active: true,
                use_wrong_gmp_account: false,
                token_paused: false,
                rate_limit_exceeded: false,
            };

            match case {
                MintErrorCase::ZeroAmount => Self {
                    amount: 0,
                    ..default
                },
                MintErrorCase::ReceiverMismatch => Self {
                    use_wrong_receiver: true,
                    ..default
                },
                MintErrorCase::GmpNotSigner => Self {
                    gmp_is_signer: false,
                    ..default
                },
                MintErrorCase::BridgeNotActive => Self {
                    bridge_active: false,
                    ..default
                },
                MintErrorCase::InvalidGmpAccount => Self {
                    use_wrong_gmp_account: true,
                    ..default
                },
                MintErrorCase::TokenPaused => Self {
                    token_paused: true,
                    ..default
                },
                MintErrorCase::MintRateLimitExceeded => Self {
                    rate_limit_exceeded: true,
                    ..default
                },
            }
        }
    }

    fn run_mint_error_test(case: MintErrorCase) {
        let config = MintTestConfig::from(case);
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let receiver = Pubkey::new_unique();
        let wrong_receiver = Pubkey::new_unique();
        let payer = Pubkey::new_unique();
        let gmp_program = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda();
        let (app_mint_state_pda, app_mint_state_bump) = get_app_mint_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (gmp_account_pda, _) =
            get_gmp_account_pda(TEST_CLIENT_ID, TEST_COUNTERPARTY_ADDRESS, &gmp_program);
        let wrong_gmp_account = Pubkey::new_unique();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account_with_options(
            app_state_bump,
            Pubkey::new_unique(),
            gmp_program,
            config.token_paused,
        );
        let app_mint_state_account = if config.rate_limit_exceeded {
            // Set daily limit to 100 with usage already at 100, so any mint exceeds the limit
            create_ift_app_mint_state_account_full(IftAppMintStateParams {
                mint,
                bump: app_mint_state_bump,
                mint_authority_bump,
                daily_mint_limit: 100,
                rate_limit_day: 0,
                rate_limit_daily_usage: 100,
            })
        } else {
            create_ift_app_mint_state_account(mint, app_mint_state_bump, mint_authority_bump)
        };

        let ift_bridge_account = create_ift_bridge_account(
            mint,
            TEST_CLIENT_ID,
            TEST_COUNTERPARTY_ADDRESS,
            ChainOptions::Evm,
            ift_bridge_bump,
            config.bridge_active,
        );

        let mint_account = solana_sdk::account::Account {
            lamports: 1_000_000,
            data: vec![0; 82],
            owner: anchor_spl::token::ID,
            executable: false,
            rent_epoch: 0,
        };

        let mint_authority_account = solana_sdk::account::Account {
            lamports: 0,
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let token_account_owner = if config.use_wrong_receiver {
            wrong_receiver
        } else {
            receiver
        };

        let receiver_token_pda = Pubkey::new_unique();
        let mut receiver_token_data = vec![0u8; 165];
        receiver_token_data[0..32].copy_from_slice(&mint.to_bytes());
        receiver_token_data[32..64].copy_from_slice(&token_account_owner.to_bytes());
        let receiver_token_account = solana_sdk::account::Account {
            lamports: 1_000_000,
            data: receiver_token_data,
            owner: anchor_spl::token::ID,
            executable: false,
            rent_epoch: 0,
        };

        let token_program_account = solana_sdk::account::Account {
            lamports: 1,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        };

        let associated_token_program_account = solana_sdk::account::Account {
            lamports: 1,
            data: vec![],
            owner: solana_sdk::native_loader::ID,
            executable: true,
            rent_epoch: 0,
        };

        let gmp_account_key = if config.use_wrong_gmp_account {
            wrong_gmp_account
        } else {
            gmp_account_pda
        };

        let receiver_owner_key = if config.use_wrong_receiver {
            wrong_receiver
        } else {
            receiver
        };

        let msg = IFTMintMsg {
            receiver,
            amount: config.amount,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(app_state_pda, false),
                AccountMeta::new(app_mint_state_pda, false),
                AccountMeta::new_readonly(ift_bridge_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(receiver_token_pda, false),
                AccountMeta::new_readonly(receiver_owner_key, false),
                AccountMeta::new_readonly(gmp_account_key, config.gmp_is_signer),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(anchor_spl::token::ID, false),
                AccountMeta::new_readonly(anchor_spl::associated_token::ID, false),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::IftMint { msg }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (app_mint_state_pda, app_mint_state_account),
            (ift_bridge_pda, ift_bridge_account),
            (mint, mint_account),
            (mint_authority_pda, mint_authority_account),
            (receiver_token_pda, receiver_token_account),
            (receiver_owner_key, create_signer_account()),
            (gmp_account_key, create_signer_account()),
            (payer, create_signer_account()),
            (anchor_spl::token::ID, token_program_account),
            (
                anchor_spl::associated_token::ID,
                associated_token_program_account,
            ),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[rstest]
    #[case::zero_amount(MintErrorCase::ZeroAmount)]
    #[case::receiver_mismatch(MintErrorCase::ReceiverMismatch)]
    #[case::gmp_not_signer(MintErrorCase::GmpNotSigner)]
    #[case::bridge_not_active(MintErrorCase::BridgeNotActive)]
    #[case::invalid_gmp_account(MintErrorCase::InvalidGmpAccount)]
    #[case::token_paused(MintErrorCase::TokenPaused)]
    #[case::mint_rate_limit_exceeded(MintErrorCase::MintRateLimitExceeded)]
    fn test_ift_mint_validation(#[case] case: MintErrorCase) {
        run_mint_error_test(case);
    }
}
