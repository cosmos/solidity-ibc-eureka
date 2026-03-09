use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::set_return_data;

use crate::state::ConsensusStateStore;
use crate::types::ClientState;

/// Returns the client status (active, frozen, or expired) via return data.
#[derive(Accounts)]
pub struct ClientStatus<'info> {
    /// The attestation client state to check for frozen status.
    #[account(
        seeds = [ClientState::SEED],
        bump
    )]
    pub client_state: Account<'info, ClientState>,
    /// The consensus state at the client's latest height (validates the client is initialized).
    #[account(
        seeds = [ConsensusStateStore::SEED, &client_state.latest_height.to_le_bytes()],
        bump
    )]
    pub consensus_state: Account<'info, ConsensusStateStore>,
}

pub fn client_status(ctx: Context<ClientStatus>) -> Result<()> {
    let client_state = &ctx.accounts.client_state;
    let status = if client_state.is_frozen {
        ics25_handler::ClientStatus::Frozen
    } else {
        let clock = Clock::get()?;
        let consensus_ts = ctx.accounts.consensus_state.timestamp as i64;
        let elapsed = clock.unix_timestamp.saturating_sub(consensus_ts);
        if elapsed > client_state.trusting_period as i64 {
            ics25_handler::ClientStatus::Expired
        } else {
            ics25_handler::ClientStatus::Active
        }
    };
    set_return_data(&[status.into()]);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::accounts::create_consensus_state_account;
    use crate::test_helpers::fixtures::*;
    use crate::test_helpers::PROGRAM_BINARY_PATH;
    use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
    use anchor_lang::AccountSerialize;
    use anchor_lang::InstructionData;
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;

    const LATEST_HEIGHT: u64 = 100;

    fn setup_client_status_test(
        frozen: bool,
    ) -> (Instruction, Vec<(solana_sdk::pubkey::Pubkey, Account)>) {
        let mut client_state = default_client_state(LATEST_HEIGHT);
        client_state.is_frozen = frozen;

        let client_state_pda = ClientState::pda();
        let consensus_state_pda = ConsensusStateStore::pda(LATEST_HEIGHT);

        let mut client_data = vec![];
        client_state.try_serialize(&mut client_data).unwrap();

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new_readonly(client_state_pda, false),
                AccountMeta::new_readonly(consensus_state_pda, false),
            ],
            data: crate::instruction::ClientStatus {}.data(),
        };

        let accounts = vec![
            (
                client_state_pda,
                Account {
                    lamports: 1_000_000,
                    data: client_data,
                    owner: crate::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                consensus_state_pda,
                create_consensus_state_account(LATEST_HEIGHT, DEFAULT_TIMESTAMP),
            ),
        ];

        (instruction, accounts)
    }

    fn run_client_status_test(frozen: bool, clock_timestamp: i64) -> Vec<u8> {
        let (instruction, accounts) = setup_client_status_test(frozen);
        let mut mollusk = Mollusk::new(&crate::ID, PROGRAM_BINARY_PATH);
        mollusk.sysvars.clock.unix_timestamp = clock_timestamp;
        mollusk.sysvars.clock.slot = 1;
        let checks = vec![Check::success()];
        let result = mollusk.process_and_validate_instruction(&instruction, &accounts, &checks);
        result.return_data
    }

    #[test]
    fn test_client_status_active() {
        // Clock just after consensus timestamp — within trusting period
        let clock_ts = DEFAULT_TIMESTAMP as i64 + 1;
        let return_data = run_client_status_test(false, clock_ts);
        assert_eq!(
            return_data,
            vec![u8::from(ics25_handler::ClientStatus::Active)]
        );
    }

    #[test]
    fn test_client_status_frozen() {
        let clock_ts = DEFAULT_TIMESTAMP as i64 + 1;
        let return_data = run_client_status_test(true, clock_ts);
        assert_eq!(
            return_data,
            vec![u8::from(ics25_handler::ClientStatus::Frozen)]
        );
    }

    #[test]
    fn test_client_status_expired() {
        // Clock past consensus timestamp + trusting period
        let clock_ts = DEFAULT_TIMESTAMP as i64 + DEFAULT_TRUSTING_PERIOD as i64 + 1;
        let return_data = run_client_status_test(false, clock_ts);
        assert_eq!(
            return_data,
            vec![u8::from(ics25_handler::ClientStatus::Expired)]
        );
    }

    #[test]
    fn test_client_status_active_at_boundary() {
        // Clock exactly at consensus timestamp + trusting period — should still be active
        let clock_ts = DEFAULT_TIMESTAMP as i64 + DEFAULT_TRUSTING_PERIOD as i64;
        let return_data = run_client_status_test(false, clock_ts);
        assert_eq!(
            return_data,
            vec![u8::from(ics25_handler::ClientStatus::Active)]
        );
    }
}
