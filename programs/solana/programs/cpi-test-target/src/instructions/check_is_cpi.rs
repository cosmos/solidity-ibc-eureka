use anchor_lang::prelude::*;
use solana_ibc_types::cpi::is_cpi;

#[account]
pub struct CpiResult {
    pub value: u8,
}

const CPI_RESULT_SEED: &[u8] = b"cpi_result";

#[derive(Accounts)]
pub struct CheckIsCpi<'info> {
    #[account(
        init_if_needed,
        payer = payer,
        space = 8 + 1,
        seeds = [CPI_RESULT_SEED],
        bump,
    )]
    pub result: Account<'info, CpiResult>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn check_is_cpi(ctx: Context<CheckIsCpi>) -> Result<()> {
    // PDA is needed because CPI tests wrap this in proxy_cpi, which
    // overwrites return_data. A persisted account survives the outer call.
    let result_account = &mut ctx.accounts.result;
    result_account.value = u8::from(is_cpi());
    Ok(())
}

#[cfg(test)]
mod tests {
    use anchor_lang::{AnchorDeserialize, InstructionData, ToAccountMetas};
    use rstest::{fixture, rstest};
    use solana_sdk::{instruction::Instruction, pubkey::Pubkey, signer::Signer};

    use crate::test_utils::*;

    #[fixture]
    async fn ctx() -> TestContext {
        let pt = setup_program_test();
        let (banks_client, payer, recent_blockhash) = pt.start().await;
        TestContext {
            banks_client,
            payer,
            recent_blockhash,
        }
    }

    fn build_ix(payer: Pubkey) -> Instruction {
        let (result_pda, _) = Pubkey::find_program_address(&[b"cpi_result"], &crate::ID);
        let accounts = crate::accounts::CheckIsCpi {
            result: result_pda,
            payer,
            system_program: solana_sdk::system_program::ID,
        };
        Instruction {
            program_id: crate::ID,
            accounts: accounts.to_account_metas(None),
            data: crate::instruction::CheckIsCpi {}.data(),
        }
    }

    async fn read_cpi_result(banks_client: &solana_program_test::BanksClient, pda: Pubkey) -> u8 {
        let account = banks_client
            .get_account(pda)
            .await
            .unwrap()
            .expect("CpiResult PDA not found");
        super::CpiResult::deserialize(&mut &account.data[8..])
            .unwrap()
            .value
    }

    #[rstest]
    #[tokio::test]
    async fn test_is_cpi_false_for_direct_call(#[future] ctx: TestContext) {
        let TestContext {
            banks_client,
            payer,
            recent_blockhash,
        } = ctx.await;

        let ix = build_ix(payer.pubkey());
        process_tx(&banks_client, &payer, recent_blockhash, &[ix])
            .await
            .unwrap();

        let (result_pda, _) = Pubkey::find_program_address(&[b"cpi_result"], &crate::ID);
        let value = read_cpi_result(&banks_client, result_pda).await;
        assert_eq!(value, 0, "is_cpi() should be false for a direct call");
    }

    #[rstest]
    #[tokio::test]
    async fn test_is_cpi_true_for_cpi_call(#[future] ctx: TestContext) {
        let TestContext {
            banks_client,
            payer,
            recent_blockhash,
        } = ctx.await;

        let inner_ix = build_ix(payer.pubkey());
        let ix = build_single_cpi_ix(payer.pubkey(), &inner_ix);
        process_tx(&banks_client, &payer, recent_blockhash, &[ix])
            .await
            .unwrap();

        let (result_pda, _) = Pubkey::find_program_address(&[b"cpi_result"], &crate::ID);
        let value = read_cpi_result(&banks_client, result_pda).await;
        assert_eq!(value, 1, "is_cpi() should be true for a CPI call");
    }

    #[rstest]
    #[tokio::test]
    async fn test_is_cpi_true_for_nested_cpi(#[future] ctx: TestContext) {
        let TestContext {
            banks_client,
            payer,
            recent_blockhash,
        } = ctx.await;

        let inner_ix = build_ix(payer.pubkey());
        let ix = build_nested_cpi_ix(payer.pubkey(), &inner_ix);
        process_tx(&banks_client, &payer, recent_blockhash, &[ix])
            .await
            .unwrap();

        let (result_pda, _) = Pubkey::find_program_address(&[b"cpi_result"], &crate::ID);
        let value = read_cpi_result(&banks_client, result_pda).await;
        assert_eq!(value, 1, "is_cpi() should be true for nested CPI");
    }
}
