use crate::{
    error::ErrorCode,
    helpers::{decode_state_attestation, AttestationProof},
    state::{ClientState, ConsensusStateStore, UpdateResult},
    UpdateClient,
};
use anchor_lang::{
    prelude::*,
    system_program::{self, CreateAccount},
};

/// Handler for the update_client instruction
/// Implements the logic from AttestationLightClient.sol:88-122
pub fn handler(ctx: Context<UpdateClient>, update_msg: Vec<u8>) -> Result<UpdateResult> {
    // Check if client is frozen
    require!(!ctx.accounts.client_state.is_frozen(), ErrorCode::ClientFrozen);

    // Step 1: Decode AttestationProof from updateMsg
    let proof: AttestationProof = serde_json::from_slice(&update_msg)
        .map_err(|_| ErrorCode::JsonDeserializationFailed)?;

    // Step 2: Compute SHA256 digest of proof.attestationData
    // TODO: CRITICAL - This step is currently skipped
    // The Solidity implementation computes: bytes32 digest = sha256(proof.attestationData);
    // See: AttestationLightClient.sol:97
    // let digest = sha256_digest(&proof.attestation_data);

    // Step 3: Verify signatures using _verifySignaturesThreshold logic
    // TODO: CRITICAL - This step is currently skipped
    // The Solidity implementation verifies attestor signatures by:
    // 1. Checking signatures.len() >= min_required_sigs
    // 2. For each signature: recover signer, verify in attestor set, check duplicates
    // See: AttestationLightClient.sol:98, 214-233
    // verify_signatures_threshold(
    //     &digest,
    //     &proof.signatures,
    //     &ctx.accounts.client_state.attestor_addresses,
    //     ctx.accounts.client_state.min_required_sigs,
    // )?;

    // Step 4: Decode StateAttestation from proof.attestationData
    let state = decode_state_attestation(&proof.attestation_data)?;

    // Step 5: Validate state.height > 0 && state.timestamp > 0
    require!(
        state.height > 0 && state.timestamp > 0,
        ErrorCode::InvalidState
    );

    // Step 6: Check if consensus state already exists at this height
    let result = store_consensus_state(StoreConsensusStateParams {
        account: &ctx.accounts.consensus_state,
        submitter: &ctx.accounts.payer,
        system_program: &ctx.accounts.system_program,
        client_key: ctx.accounts.client_state.key(),
        height: state.height,
        timestamp: state.timestamp,
        client_state: &mut ctx.accounts.client_state,
    })?;

    Ok(result)
}

struct StoreConsensusStateParams<'a, 'info> {
    account: &'a AccountInfo<'info>,
    submitter: &'a Signer<'info>,
    system_program: &'a Program<'info, System>,
    client_key: Pubkey,
    height: u64,
    timestamp: u64,
    client_state: &'a mut Account<'info, ClientState>,
}

/// Store consensus state, handling existing state conflicts
/// Implements the logic from AttestationLightClient.sol:105-120
fn store_consensus_state(params: StoreConsensusStateParams) -> Result<UpdateResult> {
    // Validate PDA
    let (expected_pda, bump) = Pubkey::find_program_address(
        &[
            ConsensusStateStore::SEED,
            params.client_key.as_ref(),
            &params.height.to_le_bytes(),
        ],
        &crate::ID,
    );

    require_keys_eq!(
        expected_pda,
        params.account.key(),
        ErrorCode::InvalidProof
    );

    // Check if account exists
    if params.account.lamports() > 0 {
        // Account exists - check for conflicts
        let account_data = params.account.try_borrow_data()?;
        if !account_data.is_empty() {
            let existing: ConsensusStateStore =
                ConsensusStateStore::try_deserialize(&mut &account_data[..])?;

            // Check if timestamp matches
            // See: AttestationLightClient.sol:108-113
            if existing.timestamp != params.timestamp {
                // Misbehaviour detected: same height but different timestamp
                // Freeze the client
                params.client_state.freeze();
                msg!(
                    "Misbehaviour detected at height {}: existing timestamp {}, new timestamp {}",
                    params.height,
                    existing.timestamp,
                    params.timestamp
                );
                return Ok(UpdateResult::Misbehaviour);
            }

            // Same height and timestamp - no operation needed
            msg!(
                "Consensus state already exists at height {} with matching timestamp {}",
                params.height,
                params.timestamp
            );
            return Ok(UpdateResult::NoOp);
        }
    }

    // Create new consensus state account
    // See: AttestationLightClient.sol:119
    let space = 8 + ConsensusStateStore::INIT_SPACE;
    let rent = Rent::get()?.minimum_balance(space);

    let seeds_with_bump = [
        ConsensusStateStore::SEED,
        params.client_key.as_ref(),
        &params.height.to_le_bytes(),
        &[bump],
    ];

    let cpi_accounts = CreateAccount {
        from: params.submitter.to_account_info(),
        to: params.account.to_account_info(),
    };
    let cpi_program = params.system_program.to_account_info();
    let signer = &[&seeds_with_bump[..]];
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);

    system_program::create_account(cpi_ctx, rent, space as u64, &crate::ID)?;

    // Serialize the new consensus state
    let new_store = ConsensusStateStore {
        height: params.height,
        timestamp: params.timestamp,
    };

    let mut data = params.account.try_borrow_mut_data()?;
    let mut cursor = std::io::Cursor::new(&mut data[..]);
    new_store.try_serialize(&mut cursor)?;

    // Step 7: Update clientState.latestHeight if new height is higher
    // See: AttestationLightClient.sol:116-118
    if params.height > params.client_state.latest_height {
        params.client_state.latest_height = params.height;
        msg!(
            "Updated client latest height from {} to {}",
            params.client_state.latest_height,
            params.height
        );
    }

    msg!(
        "Successfully created consensus state at height {} with timestamp {}",
        params.height,
        params.timestamp
    );

    // Step 8: Return UpdateResult::Update
    Ok(UpdateResult::Update)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_result_serialization() {
        // Verify UpdateResult enum matches Solidity ordering
        assert_eq!(UpdateResult::Update as u8, 0);
        assert_eq!(UpdateResult::Misbehaviour as u8, 1);
        assert_eq!(UpdateResult::NoOp as u8, 2);
    }
}
