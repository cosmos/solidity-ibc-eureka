use anchor_lang::prelude::*;

use crate::constants::*;
use crate::errors::IFTError;
use crate::events::IFTBridgeRegistered;
use crate::state::{
    AccountVersion, CounterpartyChainType, IFTAppState, IFTBridge, RegisterIFTBridgeMsg,
};

#[derive(Accounts)]
#[instruction(msg: RegisterIFTBridgeMsg)]
pub struct RegisterIFTBridge<'info> {
    #[account(
        mut,
        seeds = [IFT_APP_STATE_SEED, app_state.mint.as_ref()],
        bump = app_state.bump,
    )]
    pub app_state: Account<'info, IFTAppState>,

    /// IFT bridge PDA (to be created)
    #[account(
        init,
        payer = payer,
        space = 8 + IFTBridge::INIT_SPACE,
        seeds = [IFT_BRIDGE_SEED, app_state.mint.as_ref(), msg.client_id.as_bytes()],
        bump
    )]
    pub ift_bridge: Account<'info, IFTBridge>,

    /// CHECK: Validated via seeds constraint using stored `access_manager` program ID
    #[account(
        seeds = [access_manager::state::AccessManager::SEED],
        bump,
        seeds::program = app_state.access_manager
    )]
    pub access_manager: AccountInfo<'info>,

    /// Authority with admin role
    pub authority: Signer<'info>,

    /// Instructions sysvar for CPI validation
    /// CHECK: Address constraint verifies this is the instructions sysvar
    #[account(address = anchor_lang::solana_program::sysvar::instructions::ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn register_ift_bridge(
    ctx: Context<RegisterIFTBridge>,
    msg: RegisterIFTBridgeMsg,
) -> Result<()> {
    access_manager::require_role(
        &ctx.accounts.access_manager,
        solana_ibc_types::roles::ADMIN_ROLE,
        &ctx.accounts.authority,
        &ctx.accounts.instructions_sysvar,
        &crate::ID,
    )?;

    require!(!msg.client_id.is_empty(), IFTError::EmptyClientId);
    require!(
        msg.client_id.len() <= MAX_CLIENT_ID_LENGTH,
        IFTError::InvalidClientIdLength
    );
    require!(
        !msg.counterparty_ift_address.is_empty(),
        IFTError::EmptyCounterpartyAddress
    );
    require!(
        msg.counterparty_ift_address.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCounterpartyAddressLength
    );

    if msg.counterparty_chain_type == CounterpartyChainType::Cosmos {
        require!(
            !msg.counterparty_denom.is_empty(),
            IFTError::CosmosEmptyCounterpartyDenom
        );
        require!(
            !msg.cosmos_type_url.is_empty(),
            IFTError::CosmosEmptyTypeUrl
        );
        require!(
            !msg.cosmos_ica_address.is_empty(),
            IFTError::CosmosEmptyIcaAddress
        );
    }
    require!(
        msg.counterparty_denom.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCounterpartyDenomLength
    );
    require!(
        msg.cosmos_type_url.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCosmosTypeUrlLength
    );
    require!(
        msg.cosmos_ica_address.len() <= MAX_COUNTERPARTY_ADDRESS_LENGTH,
        IFTError::InvalidCosmosIcaAddressLength
    );

    let bridge = &mut ctx.accounts.ift_bridge;
    bridge.version = AccountVersion::V1;
    bridge.bump = ctx.bumps.ift_bridge;
    bridge.mint = ctx.accounts.app_state.mint;
    bridge.client_id.clone_from(&msg.client_id);
    bridge
        .counterparty_ift_address
        .clone_from(&msg.counterparty_ift_address);
    bridge
        .counterparty_denom
        .clone_from(&msg.counterparty_denom);
    bridge.cosmos_type_url.clone_from(&msg.cosmos_type_url);
    bridge
        .cosmos_ica_address
        .clone_from(&msg.cosmos_ica_address);
    bridge.counterparty_chain_type = msg.counterparty_chain_type;
    bridge.active = true;

    let clock = Clock::get()?;
    emit!(IFTBridgeRegistered {
        mint: ctx.accounts.app_state.mint,
        client_id: msg.client_id,
        counterparty_ift_address: msg.counterparty_ift_address,
        counterparty_denom: msg.counterparty_denom,
        cosmos_type_url: msg.cosmos_type_url,
        cosmos_ica_address: msg.cosmos_ica_address,
        counterparty_chain_type: msg.counterparty_chain_type,
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{InstructionData, Space};
    use solana_sdk::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        rent::Rent,
    };
    use test_case::test_case;

    use crate::state::{CounterpartyChainType, IFTBridge, RegisterIFTBridgeMsg};
    use crate::test_utils::*;

    const TEST_CLIENT_ID: &str = "07-tendermint-0";
    const TEST_COUNTERPARTY_ADDRESS: &str = "0x1234567890abcdef1234567890abcdef12345678";

    #[test]
    fn test_register_ift_bridge_success() {
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (bridge_pda, _) = get_bridge_pda(&mint, TEST_CLIENT_ID);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let bridge_account = solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(8 + IFTBridge::INIT_SPACE),
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let msg = RegisterIFTBridgeMsg {
            client_id: TEST_CLIENT_ID.to_string(),
            counterparty_ift_address: TEST_COUNTERPARTY_ADDRESS.to_string(),
            counterparty_denom: String::new(),
            cosmos_type_url: String::new(),
            cosmos_ica_address: String::new(),
            counterparty_chain_type: CounterpartyChainType::Evm,
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(bridge_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(admin, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::RegisterIftBridge { msg }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (bridge_pda, bridge_account),
            (access_manager_pda, access_manager_account),
            (admin, create_signer_account()),
            (instructions_sysvar, instructions_account),
            (payer, create_signer_account()),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(
            !result.program_result.is_err(),
            "register_ift_bridge should succeed: {:?}",
            result.program_result
        );

        let bridge_account = result
            .resulting_accounts
            .iter()
            .find(|(k, _)| *k == bridge_pda)
            .expect("bridge should exist")
            .1
            .clone();

        assert_eq!(
            bridge_account.owner,
            crate::ID,
            "Bridge should be owned by IFT program"
        );
        let bridge = deserialize_bridge(&bridge_account);
        assert_eq!(bridge.client_id, TEST_CLIENT_ID);
        assert_eq!(bridge.counterparty_ift_address, TEST_COUNTERPARTY_ADDRESS);
        assert!(bridge.active);
    }

    #[derive(Clone, Copy)]
    enum RegisterBridgeErrorCase {
        EmptyClientId,
        EmptyCounterparty,
        Unauthorized,
        ClientIdTooLong,
        CounterpartyTooLong,
    }

    struct RegisterBridgeTestConfig {
        client_id: String,
        counterparty_address: String,
        use_unauthorized_signer: bool,
    }

    impl From<RegisterBridgeErrorCase> for RegisterBridgeTestConfig {
        fn from(case: RegisterBridgeErrorCase) -> Self {
            match case {
                RegisterBridgeErrorCase::EmptyClientId => Self {
                    client_id: String::new(),
                    counterparty_address: "0x1234".to_string(),
                    use_unauthorized_signer: false,
                },
                RegisterBridgeErrorCase::EmptyCounterparty => Self {
                    client_id: TEST_CLIENT_ID.to_string(),
                    counterparty_address: String::new(),
                    use_unauthorized_signer: false,
                },
                RegisterBridgeErrorCase::Unauthorized => Self {
                    client_id: TEST_CLIENT_ID.to_string(),
                    counterparty_address: "0x1234".to_string(),
                    use_unauthorized_signer: true,
                },
                RegisterBridgeErrorCase::ClientIdTooLong => Self {
                    client_id: "x".repeat(crate::constants::MAX_CLIENT_ID_LENGTH + 1),
                    counterparty_address: "0x1234".to_string(),
                    use_unauthorized_signer: false,
                },
                RegisterBridgeErrorCase::CounterpartyTooLong => Self {
                    client_id: TEST_CLIENT_ID.to_string(),
                    counterparty_address: "x"
                        .repeat(crate::constants::MAX_COUNTERPARTY_ADDRESS_LENGTH + 1),
                    use_unauthorized_signer: false,
                },
            }
        }
    }

    fn run_register_bridge_error_test(case: RegisterBridgeErrorCase) {
        let config = RegisterBridgeTestConfig::from(case);
        let mollusk = setup_mollusk();

        let mint = Pubkey::new_unique();
        let admin = Pubkey::new_unique();
        let unauthorized = Pubkey::new_unique();
        let payer = Pubkey::new_unique();

        let pda_client_id = if config.client_id.is_empty()
            || config.client_id.len() > crate::constants::MAX_CLIENT_ID_LENGTH
        {
            TEST_CLIENT_ID
        } else {
            &config.client_id
        };

        let (app_state_pda, app_state_bump) = get_app_state_pda(&mint);
        let (_, mint_authority_bump) = get_mint_authority_pda(&mint);
        let (bridge_pda, _) = get_bridge_pda(&mint, pda_client_id);
        let (access_manager_pda, access_manager_account) =
            create_access_manager_account_with_admin(admin);
        let (instructions_sysvar, instructions_account) = create_instructions_sysvar_account();
        let (system_program, system_account) = create_system_program_account();

        let app_state_account = create_ift_app_state_account(
            mint,
            app_state_bump,
            mint_authority_bump,
            access_manager::ID,
            Pubkey::new_unique(),
        );

        let bridge_account = solana_sdk::account::Account {
            lamports: Rent::default().minimum_balance(8 + IFTBridge::INIT_SPACE),
            data: vec![],
            owner: solana_sdk::system_program::ID,
            executable: false,
            rent_epoch: 0,
        };

        let msg = RegisterIFTBridgeMsg {
            client_id: config.client_id,
            counterparty_ift_address: config.counterparty_address,
            counterparty_denom: String::new(),
            cosmos_type_url: String::new(),
            cosmos_ica_address: String::new(),
            counterparty_chain_type: CounterpartyChainType::Evm,
        };

        let signer = if config.use_unauthorized_signer {
            unauthorized
        } else {
            admin
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(app_state_pda, false),
                AccountMeta::new(bridge_pda, false),
                AccountMeta::new_readonly(access_manager_pda, false),
                AccountMeta::new_readonly(signer, true),
                AccountMeta::new_readonly(instructions_sysvar, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program, false),
            ],
            data: crate::instruction::RegisterIftBridge { msg }.data(),
        };

        let accounts = vec![
            (app_state_pda, app_state_account),
            (bridge_pda, bridge_account),
            (access_manager_pda, access_manager_account),
            (signer, create_signer_account()),
            (instructions_sysvar, instructions_account),
            (payer, create_signer_account()),
            (system_program, system_account),
        ];

        let result = mollusk.process_instruction(&instruction, &accounts);
        assert!(result.program_result.is_err());
    }

    #[test_case(RegisterBridgeErrorCase::EmptyClientId ; "empty_client_id")]
    #[test_case(RegisterBridgeErrorCase::EmptyCounterparty ; "empty_counterparty")]
    #[test_case(RegisterBridgeErrorCase::Unauthorized ; "unauthorized")]
    #[test_case(RegisterBridgeErrorCase::ClientIdTooLong ; "client_id_too_long")]
    #[test_case(RegisterBridgeErrorCase::CounterpartyTooLong ; "counterparty_too_long")]
    fn test_register_ift_bridge_validation(case: RegisterBridgeErrorCase) {
        run_register_bridge_error_test(case);
    }
}
