use crate::error::ErrorCode;
use crate::types::{ClientState, ConsensusState};
use crate::Initialize;
use anchor_lang::prelude::*;

pub fn initialize(
    ctx: Context<Initialize>,
    client_state: ClientState,
    consensus_state: ConsensusState,
) -> Result<()> {
    require!(!client_state.chain_id.is_empty(), ErrorCode::InvalidChainId);

    require!(
        client_state.trust_level_numerator > 0
            && client_state.trust_level_numerator <= client_state.trust_level_denominator
            && client_state.trust_level_denominator > 0,
        ErrorCode::InvalidTrustLevel
    );

    require!(
        client_state.trusting_period > 0
            && client_state.unbonding_period > 0
            && client_state.trusting_period < client_state.unbonding_period,
        ErrorCode::InvalidPeriods
    );

    require!(
        client_state.max_clock_drift > 0,
        ErrorCode::InvalidMaxClockDrift
    );

    require!(
        client_state.latest_height.revision_height > 0,
        ErrorCode::InvalidHeight
    );

    let client_state_account = &mut ctx.accounts.client_state;
    let latest_height = client_state.latest_height;
    client_state_account.set_inner(client_state);

    let consensus_state_store = &mut ctx.accounts.consensus_state_store;
    consensus_state_store.height = latest_height.revision_height;
    consensus_state_store.consensus_state = consensus_state;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ConsensusStateStore;
    use crate::test_helpers::fixtures::*;
    use anchor_lang::{AnchorDeserialize, InstructionData};
    use mollusk_svm::result::Check;
    use mollusk_svm::Mollusk;
    use solana_sdk::account::Account;
    use solana_sdk::instruction::{AccountMeta, Instruction};
    use solana_sdk::pubkey::Pubkey;
    use solana_sdk::{native_loader, system_program};

    #[test]
    fn test_initialize_happy_path() {
        // Load all fixtures efficiently (single JSON parse)
        let (client_state, consensus_state, _) = load_primary_fixtures();
        
        let chain_id = &client_state.chain_id;

        let payer = Pubkey::new_unique();

        let (client_state_pda, _) =
            Pubkey::find_program_address(&[b"client", chain_id.as_bytes()], &crate::ID);

        let latest_height = client_state.latest_height.revision_height;
        let (consensus_state_store_pda, _) = Pubkey::find_program_address(
            &[
                b"consensus_state",
                client_state_pda.as_ref(),
                &latest_height.to_le_bytes(),
            ],
            &crate::ID,
        );

        let instruction_data = crate::instruction::Initialize {
            chain_id: chain_id.to_string(),
            latest_height,
            client_state: client_state.clone(),
            consensus_state: consensus_state.clone(),
        };

        let instruction = Instruction {
            program_id: crate::ID,
            accounts: vec![
                AccountMeta::new(client_state_pda, false),
                AccountMeta::new(consensus_state_store_pda, false),
                AccountMeta::new(payer, true),
                AccountMeta::new_readonly(system_program::ID, false),
            ],
            data: instruction_data.data(),
        };

        let payer_lamports = 10_000_000_000;
        let accounts = vec![
            (
                client_state_pda,
                Account {
                    lamports: 0,
                    data: vec![],
                    owner: system_program::ID,
                    executable: false,
                    rent_epoch: 0,
                },
            ),
            (
                consensus_state_store_pda,
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

        let mollusk = Mollusk::new(&crate::ID, "../../target/deploy/ics07_tendermint");

        let checks = vec![
            Check::success(),
            Check::account(&client_state_pda).owner(&crate::ID).build(),
            Check::account(&consensus_state_store_pda)
                .owner(&crate::ID)
                .build(),
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

        let client_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &client_state_pda)
            .map(|(_, account)| account)
            .expect("Client state account not found");

        assert!(
            client_state_account.lamports > 0,
            "Client state account should be rent-exempt"
        );
        assert!(
            client_state_account.data.len() > 8,
            "Client state account should have data"
        );

        let mut data_slice = &client_state_account.data[8..];
        let deserialized_client_state: ClientState =
            ClientState::deserialize(&mut data_slice).expect("Failed to deserialize client state");

        assert_eq!(deserialized_client_state.chain_id, client_state.chain_id);
        assert_eq!(
            deserialized_client_state.trust_level_numerator,
            client_state.trust_level_numerator
        );
        assert_eq!(
            deserialized_client_state.trust_level_denominator,
            client_state.trust_level_denominator
        );
        assert_eq!(
            deserialized_client_state.trusting_period,
            client_state.trusting_period
        );
        assert_eq!(
            deserialized_client_state.unbonding_period,
            client_state.unbonding_period
        );
        assert_eq!(
            deserialized_client_state.max_clock_drift,
            client_state.max_clock_drift
        );
        assert_eq!(
            deserialized_client_state.frozen_height.revision_number,
            client_state.frozen_height.revision_number
        );
        assert_eq!(
            deserialized_client_state.frozen_height.revision_height,
            client_state.frozen_height.revision_height
        );
        assert_eq!(
            deserialized_client_state.latest_height.revision_number,
            client_state.latest_height.revision_number
        );
        assert_eq!(
            deserialized_client_state.latest_height.revision_height,
            client_state.latest_height.revision_height
        );

        let consensus_state_account = result
            .resulting_accounts
            .iter()
            .find(|(pubkey, _)| pubkey == &consensus_state_store_pda)
            .map(|(_, account)| account)
            .expect("Consensus state store account not found");

        assert!(
            consensus_state_account.lamports > 0,
            "Consensus state store account should be rent-exempt"
        );
        assert!(
            consensus_state_account.data.len() > 8,
            "Consensus state store account should have data"
        );

        let mut data_slice = &consensus_state_account.data[8..];
        let deserialized_consensus_store: ConsensusStateStore =
            ConsensusStateStore::deserialize(&mut data_slice)
                .expect("Failed to deserialize consensus state store");

        assert_eq!(deserialized_consensus_store.height, latest_height);
        assert_eq!(
            deserialized_consensus_store.consensus_state.timestamp,
            consensus_state.timestamp
        );
        assert_eq!(
            deserialized_consensus_store.consensus_state.root,
            consensus_state.root
        );
        assert_eq!(
            deserialized_consensus_store
                .consensus_state
                .next_validators_hash,
            consensus_state.next_validators_hash
        );
    }
}
