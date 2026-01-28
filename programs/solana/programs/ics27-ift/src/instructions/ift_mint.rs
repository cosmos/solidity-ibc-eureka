use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTMintReceived;
use crate::helpers::mint_to_account;
use crate::state::{IFTAppState, IFTBridge, IFTMintMsg};

/// IFT Mint instruction - called by GMP via CPI when receiving a cross-chain mint request
#[derive(Accounts)]
#[instruction(msg: IFTMintMsg)]
pub struct IFTMint<'info> {
    #[account(
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge - provides counterparty info for GMP account validation
    #[account(
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), msg.client_id.as_bytes()],
        bump = ift_bridge.bump,
        constraint = ift_bridge.active @ IFTError::BridgeNotActive
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// SPL Token mint
    #[account(
        mut,
        address = app_state.mint
    )]
    pub mint: Account<'info, Mint>,

    /// Mint authority PDA
    /// CHECK: Derived PDA that signs for minting
    #[account(
        seeds = [MINT_AUTHORITY_SEED, mint.key().as_ref()],
        // Use stored bump to avoid expensive `find_program_address` computation.
        // The bump is needed for PDA signing during the mint CPI call.
        bump = app_state.mint_authority_bump
    )]
    pub mint_authority: AccountInfo<'info>,

    /// Receiver's token account (will be created if needed)
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = receiver_owner
    )]
    pub receiver_token_account: Account<'info, TokenAccount>,

    // TODO: make just a receiver
    /// CHECK: The receiver owner pubkey (must match msg.receiver)
    #[account(
        constraint = receiver_owner.key() == msg.receiver @ IFTError::InvalidReceiver
    )]
    pub receiver_owner: AccountInfo<'info>,

    /// CHECK: GMP program for PDA derivation
    #[account(address = app_state.gmp_program @ IFTError::InvalidGmpProgram)]
    pub gmp_program: AccountInfo<'info>,

    /// GMP account PDA - validated to match counterparty bridge
    pub gmp_account: Signer<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn ift_mint(ctx: Context<IFTMint>, msg: IFTMintMsg) -> Result<()> {
    require!(msg.amount > 0, IFTError::ZeroAmount);
    validate_gmp_account(
        &ctx.accounts.gmp_account.key(),
        &msg.client_id,
        &ctx.accounts.ift_bridge.counterparty_ift_address,
        &ctx.accounts.gmp_program.key(),
        msg.gmp_account_bump,
    )?;

    mint_to_account(
        &ctx.accounts.mint,
        &ctx.accounts.receiver_token_account,
        &ctx.accounts.mint_authority,
        ctx.accounts.app_state.mint_authority_bump,
        &ctx.accounts.token_program,
        msg.amount,
    )?;

    let clock = Clock::get()?;
    emit!(IFTMintReceived {
        mint: ctx.accounts.mint.key(),
        client_id: msg.client_id,
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
    bump: u8,
) -> Result<()> {
    use solana_ibc_types::ics27::{AccountIdentifier, GMPAccount, Salt};

    let account_id = AccountIdentifier::new(
        client_id
            .to_string()
            .try_into()
            .map_err(|_| IFTError::InvalidGmpAccount)?,
        counterparty_address
            .to_string()
            .try_into()
            .map_err(|_| IFTError::InvalidGmpAccount)?,
        Salt::empty(),
    );

    let expected_pda = Pubkey::create_program_address(
        &[GMPAccount::SEED, &account_id.digest(), &[bump]],
        gmp_program,
    )
    .map_err(|_| IFTError::InvalidGmpAccount)?;
    require!(*gmp_account == expected_pda, IFTError::InvalidGmpAccount);
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
    }

    #[allow(clippy::struct_excessive_bools)]
    struct MintTestConfig {
        amount: u64,
        use_wrong_receiver: bool,
        gmp_is_signer: bool,
        bridge_active: bool,
        use_wrong_gmp_account: bool,
    }

    impl From<MintErrorCase> for MintTestConfig {
        fn from(case: MintErrorCase) -> Self {
            match case {
                MintErrorCase::ZeroAmount => Self {
                    amount: 0,
                    use_wrong_receiver: false,
                    gmp_is_signer: true,
                    bridge_active: true,
                    use_wrong_gmp_account: false,
                },
                MintErrorCase::ReceiverMismatch => Self {
                    amount: 1000,
                    use_wrong_receiver: true,
                    gmp_is_signer: true,
                    bridge_active: true,
                    use_wrong_gmp_account: false,
                },
                MintErrorCase::GmpNotSigner => Self {
                    amount: 1000,
                    use_wrong_receiver: false,
                    gmp_is_signer: false,
                    bridge_active: true,
                    use_wrong_gmp_account: false,
                },
                MintErrorCase::BridgeNotActive => Self {
                    amount: 1000,
                    use_wrong_receiver: false,
                    gmp_is_signer: true,
                    bridge_active: false,
                    use_wrong_gmp_account: false,
                },
                MintErrorCase::InvalidGmpAccount => Self {
                    amount: 1000,
                    use_wrong_receiver: false,
                    gmp_is_signer: true,
                    bridge_active: true,
                    use_wrong_gmp_account: true,
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

        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (mint_authority_pda, _) = get_mint_authority_pda(&mint);
        let (ift_bridge_pda, ift_bridge_bump) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (gmp_account_pda, gmp_account_bump) =
            get_gmp_account_pda(TEST_CLIENT_ID, TEST_COUNTERPARTY_ADDRESS, &gmp_program);
        let wrong_gmp_account = Pubkey::new_unique();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            gmp_program,
        );

        let ift_bridge_account = create_ift_bridge_account(
            mint,
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

        let (gmp_account_key, gmp_bump) = if config.use_wrong_gmp_account {
            (wrong_gmp_account, 255)
        } else {
            (gmp_account_pda, gmp_account_bump)
        };

        let receiver_owner_key = if config.use_wrong_receiver {
            wrong_receiver
        } else {
            receiver
        };

        let msg = IFTMintMsg {
            receiver,
            amount: config.amount,
            client_id: TEST_CLIENT_ID.to_string(),
            gmp_account_bump: gmp_bump,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new_readonly(ift_bridge_pda, false),
                AccountMeta::new(mint, false),
                AccountMeta::new_readonly(mint_authority_pda, false),
                AccountMeta::new(receiver_token_pda, false),
                AccountMeta::new_readonly(receiver_owner_key, false),
                AccountMeta::new_readonly(gmp_program, false),
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
            (ift_bridge_pda, ift_bridge_account),
            (mint, mint_account),
            (mint_authority_pda, mint_authority_account),
            (receiver_token_pda, receiver_token_account),
            (receiver_owner_key, create_signer_account()),
            (gmp_program, create_gmp_program_account()),
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
    fn test_ift_mint_validation(#[case] case: MintErrorCase) {
        run_mint_error_test(case);
    }
}
