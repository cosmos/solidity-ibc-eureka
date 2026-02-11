use anchor_lang::{InstructionData, ToAccountMetas};
use solana_program_test::{BanksClient, BanksClientError, ProgramTest};
use solana_sdk::{
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signature::Keypair,
    signer::Signer,
    transaction::Transaction,
};

const PROGRAM_BINARY_PATH: &str = "../../target/deploy/cpi_test_target";

pub const PROGRAM_A_ID: Pubkey = malicious_caller::ID;

pub fn setup_program_test() -> ProgramTest {
    if std::env::var("SBF_OUT_DIR").is_err() {
        let deploy_dir = std::path::Path::new(PROGRAM_BINARY_PATH)
            .parent()
            .expect("Invalid program path");
        std::env::set_var("SBF_OUT_DIR", deploy_dir);
    }

    let mut pt = ProgramTest::new("cpi_test_target", crate::ID, None);
    pt.add_program("malicious_caller", PROGRAM_A_ID, None);
    pt
}

pub struct TestContext {
    pub banks_client: BanksClient,
    pub payer: Keypair,
    pub recent_blockhash: Hash,
}

fn wrap_in_malicious_caller(payer: Pubkey, inner_ix: &Instruction) -> Instruction {
    let account_metas: Vec<malicious_caller::CpiAccountMeta> = inner_ix
        .accounts
        .iter()
        .map(|m| malicious_caller::CpiAccountMeta {
            is_signer: m.is_signer,
            is_writable: m.is_writable,
        })
        .collect();

    let ix_data = malicious_caller::instruction::ProxyCpi {
        instruction_data: inner_ix.data.clone(),
        account_metas,
    };

    let mut accounts = vec![
        AccountMeta::new_readonly(inner_ix.program_id, false),
        AccountMeta::new_readonly(payer, true),
    ];
    for meta in &inner_ix.accounts {
        accounts.push(if meta.is_writable {
            AccountMeta::new(meta.pubkey, false)
        } else {
            AccountMeta::new_readonly(meta.pubkey, false)
        });
    }

    Instruction {
        program_id: PROGRAM_A_ID,
        accounts,
        data: ix_data.data(),
    }
}

fn wrap_in_cpi_test_target_proxy(payer: Pubkey, inner_ix: &Instruction) -> Instruction {
    let account_metas: Vec<crate::CpiAccountMeta> = inner_ix
        .accounts
        .iter()
        .map(|m| crate::CpiAccountMeta {
            is_signer: m.is_signer,
            is_writable: m.is_writable,
        })
        .collect();

    let ix_data = crate::instruction::ProxyCpi {
        instruction_data: inner_ix.data.clone(),
        account_metas,
    };

    let accounts_struct = crate::accounts::ProxyCpi {
        target_program: inner_ix.program_id,
        payer,
    };
    let mut accounts = accounts_struct.to_account_metas(None);
    for meta in &inner_ix.accounts {
        accounts.push(if meta.is_writable {
            AccountMeta::new(meta.pubkey, false)
        } else {
            AccountMeta::new_readonly(meta.pubkey, false)
        });
    }

    Instruction {
        program_id: crate::ID,
        accounts,
        data: ix_data.data(),
    }
}

/// Wraps `inner_ix` in a single CPI hop: Tx -> A -> B
pub fn build_single_cpi_ix(payer: Pubkey, inner_ix: &Instruction) -> Instruction {
    wrap_in_malicious_caller(payer, inner_ix)
}

/// Wraps `inner_ix` in a nested CPI chain: Tx -> A -> B(`proxy_cpi`) -> B(`check_*`)
pub fn build_nested_cpi_ix(payer: Pubkey, inner_ix: &Instruction) -> Instruction {
    let proxy_wrapping_inner = wrap_in_cpi_test_target_proxy(payer, inner_ix);
    wrap_in_malicious_caller(payer, &proxy_wrapping_inner)
}

pub async fn process_tx(
    banks_client: &BanksClient,
    payer: &Keypair,
    recent_blockhash: Hash,
    instructions: &[Instruction],
) -> Result<(), BanksClientError> {
    let tx = Transaction::new_signed_with_payer(
        instructions,
        Some(&payer.pubkey()),
        &[payer],
        recent_blockhash,
    );
    banks_client.process_transaction(tx).await
}
