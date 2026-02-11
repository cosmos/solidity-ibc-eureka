use anchor_lang::prelude::*;
use solana_ibc_types::cpi::reject_cpi;

#[derive(Accounts)]
pub struct CheckRejectCpi<'info> {
    /// CHECK: Validated inside the CPI validation functions
    pub instruction_sysvar: AccountInfo<'info>,
}

pub fn check_reject_cpi(ctx: Context<CheckRejectCpi>) -> Result<()> {
    reject_cpi(&ctx.accounts.instruction_sysvar, &crate::ID).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use anchor_lang::{InstructionData, ToAccountMetas};
    use rstest::{fixture, rstest};
    use solana_sdk::{
        instruction::Instruction, pubkey::Pubkey, signer::Signer, sysvar::instructions as ix_sysvar,
    };

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

    fn build_ix() -> Instruction {
        let accounts = crate::accounts::CheckRejectCpi {
            instruction_sysvar: ix_sysvar::ID,
        };
        Instruction {
            program_id: crate::ID,
            accounts: accounts.to_account_metas(None),
            data: crate::instruction::CheckRejectCpi {}.data(),
        }
    }

    #[rstest]
    #[tokio::test]
    async fn test_reject_cpi_allows_direct(#[future] ctx: TestContext) {
        let TestContext {
            banks_client,
            payer,
            recent_blockhash,
        } = ctx.await;

        let ix = build_ix();
        process_tx(&banks_client, &payer, recent_blockhash, &[ix])
            .await
            .unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn test_reject_cpi_rejects_any_cpi(#[future] ctx: TestContext) {
        let TestContext {
            banks_client,
            payer,
            recent_blockhash,
        } = ctx.await;

        let inner_ix = build_ix();
        let ix = build_single_cpi_ix(payer.pubkey(), &inner_ix);
        let result = process_tx(&banks_client, &payer, recent_blockhash, &[ix]).await;
        assert!(result.is_err(), "reject_cpi should reject any CPI call");
    }

    #[rstest]
    #[tokio::test]
    async fn test_reject_cpi_rejects_fake_sysvar(#[future] ctx: TestContext) {
        let TestContext {
            banks_client,
            payer,
            recent_blockhash,
        } = ctx.await;

        let accounts = crate::accounts::CheckRejectCpi {
            instruction_sysvar: Pubkey::new_unique(),
        };
        let ix = Instruction {
            program_id: crate::ID,
            accounts: accounts.to_account_metas(None),
            data: crate::instruction::CheckRejectCpi {}.data(),
        };
        let result = process_tx(&banks_client, &payer, recent_blockhash, &[ix]).await;
        assert!(
            result.is_err(),
            "reject_cpi should reject fake sysvar account"
        );
    }
}
