use ibc_client_tendermint_types::error::{IntoResult, TendermintClientError};
use ibc_client_tendermint_types::{ConsensusState as ConsensusStateType, Header as TmHeader};
use ibc_core_client::context::{Convertible, ExtClientValidationContext};
use ibc_core_client::types::error::ClientError;
use ibc_core_client::types::Height;
use ibc_core_host::types::error::IdentifierError;
use ibc_core_host::types::identifiers::{ChainId, ClientId};
use ibc_core_host::types::path::ClientConsensusStatePath;
use ibc_primitives::prelude::*;
use ibc_primitives::IntoHostTime;
use tendermint::crypto::Sha256;
use tendermint::merkle::MerkleHash;
use tendermint_light_client_verifier::options::Options;
use tendermint_light_client_verifier::types::{TrustedBlockState, UntrustedBlockState};
use tendermint_light_client_verifier::Verifier;

#[cfg(feature = "solana")]
use solana_program::{log::sol_log_compute_units, msg};

pub fn verify_header<V, H>(
    ctx: &V,
    header: &TmHeader,
    client_id: &ClientId,
    chain_id: &ChainId,
    options: &Options,
    verifier: &impl Verifier,
) -> Result<(), ClientError>
where
    V: ExtClientValidationContext,
    ConsensusStateType: Convertible<V::ConsensusStateRef>,
    <ConsensusStateType as TryFrom<V::ConsensusStateRef>>::Error: Into<ClientError>,
    H: MerkleHash + Sha256 + Default,
{
    #[cfg(feature = "solana")]
    {
        msg!(
            "[ibc-rs] verify_header ENTRY: trusted_height={}, header_height={}",
            header.trusted_height.revision_height(),
            header.height().revision_height()
        );
        sol_log_compute_units();
    }

    // Checks that the header fields are valid.
    header.validate_basic::<H>()?;

    #[cfg(feature = "solana")]
    {
        msg!("[ibc-rs] Checkpoint 1: validate_basic() completed");
        sol_log_compute_units();
    }

    // The tendermint-light-client crate though works on heights that are assumed
    // to have the same revision number. We ensure this here.
    header.verify_chain_id_version_matches_height(chain_id)?;

    #[cfg(feature = "solana")]
    {
        msg!("[ibc-rs] Checkpoint 2: chain ID version verified");
        sol_log_compute_units();
    }

    // Delegate to tendermint-light-client, which contains the required checks
    // of the new header against the trusted consensus state.
    {
        let trusted_state = {
            let trusted_client_cons_state_path = ClientConsensusStatePath::new(
                client_id.clone(),
                header.trusted_height.revision_number(),
                header.trusted_height.revision_height(),
            );
            let trusted_consensus_state: ConsensusStateType = ctx
                .consensus_state(&trusted_client_cons_state_path)?
                .try_into()
                .map_err(Into::into)?;

            #[cfg(feature = "solana")]
            {
                msg!("[ibc-rs] Checkpoint 3: trusted consensus state loaded");
                sol_log_compute_units();
            }

            header.check_trusted_next_validator_set::<H>(
                &trusted_consensus_state.next_validators_hash,
            )?;

            #[cfg(feature = "solana")]
            {
                msg!("[ibc-rs] Checkpoint 4: check_trusted_next_validator_set() completed");
                sol_log_compute_units();
            }

            TrustedBlockState {
                chain_id: &chain_id.as_str().try_into().map_err(|e| {
                    IdentifierError::FailedToParse {
                        description: format!("chain ID `{chain_id}`: {e:?}"),
                    }
                })?,
                header_time: trusted_consensus_state.timestamp(),
                height: header
                    .trusted_height
                    .revision_height()
                    .try_into()
                    .map_err(|_| ClientError::FailedToVerifyHeader {
                        description: TendermintClientError::InvalidHeaderHeight(
                            header.trusted_height.revision_height(),
                        )
                        .to_string(),
                    })?,
                next_validators: &header.trusted_next_validator_set,
                next_validators_hash: trusted_consensus_state.next_validators_hash,
            }
        };

        #[cfg(feature = "solana")]
        {
            msg!("[ibc-rs] Checkpoint 5: TrustedBlockState constructed");
            sol_log_compute_units();
        }

        let untrusted_state = UntrustedBlockState {
            signed_header: &header.signed_header,
            validators: &header.validator_set,
            // NB: This will skip the
            // VerificationPredicates::next_validators_match check for the
            // untrusted state.
            next_validators: None,
        };

        #[cfg(feature = "solana")]
        {
            msg!("[ibc-rs] Checkpoint 6: UntrustedBlockState constructed");
            sol_log_compute_units();
        }

        let now = ctx.host_timestamp()?.into_host_time()?;

        #[cfg(feature = "solana")]
        {
            msg!("[ibc-rs] Checkpoint 7: host timestamp fetched");
            sol_log_compute_units();
        }

        // main header verification, delegated to the tendermint-light-client crate.
        #[cfg(feature = "solana")]
        {
            msg!("[ibc-rs] Checkpoint 8: ABOUT TO CALL verify_update_header() - signature verification starts");
            sol_log_compute_units();
        }

        verifier
            .verify_update_header(untrusted_state, trusted_state, options, now)
            .into_result()?;

        #[cfg(feature = "solana")]
        {
            msg!("[ibc-rs] Checkpoint 9: verify_update_header() COMPLETED - signature verification done");
            sol_log_compute_units();
        }
    }

    #[cfg(feature = "solana")]
    {
        msg!("[ibc-rs] verify_header EXIT: success");
        sol_log_compute_units();
    }

    Ok(())
}

/// Checks for misbehaviour upon receiving a new consensus state as part
/// of a client update.
pub fn check_for_misbehaviour_on_update<V>(
    ctx: &V,
    header: TmHeader,
    client_id: &ClientId,
    client_latest_height: &Height,
) -> Result<bool, ClientError>
where
    V: ExtClientValidationContext,
    ConsensusStateType: Convertible<V::ConsensusStateRef>,
    <ConsensusStateType as TryFrom<V::ConsensusStateRef>>::Error: Into<ClientError>,
{
    let maybe_existing_consensus_state = {
        let path_at_header_height = ClientConsensusStatePath::new(
            client_id.clone(),
            header.height().revision_number(),
            header.height().revision_height(),
        );

        ctx.consensus_state(&path_at_header_height).ok()
    };

    if let Some(existing_consensus_state) = maybe_existing_consensus_state {
        let existing_consensus_state: ConsensusStateType =
            existing_consensus_state.try_into().map_err(Into::into)?;

        let header_consensus_state = ConsensusStateType::from(header);

        // There is evidence of misbehaviour if the stored consensus state
        // is different from the new one we received.
        Ok(existing_consensus_state != header_consensus_state)
    } else {
        // If no header was previously installed, we ensure the monotonicity of timestamps.

        // 1. for all headers, the new header needs to have a larger timestamp than
        //    the “previous header”
        {
            let maybe_prev_cs = ctx.prev_consensus_state(client_id, &header.height())?;

            if let Some(prev_cs) = maybe_prev_cs {
                // New header timestamp cannot occur *before* the
                // previous consensus state's height
                let prev_cs: ConsensusStateType = prev_cs.try_into().map_err(Into::into)?;

                if header.signed_header.header().time <= prev_cs.timestamp() {
                    return Ok(true);
                }
            }
        }

        // 2. if a header comes in and is not the “last” header, then we also ensure
        //    that its timestamp is less than the “next header”
        if &header.height() < client_latest_height {
            let maybe_next_cs = ctx.next_consensus_state(client_id, &header.height())?;

            if let Some(next_cs) = maybe_next_cs {
                // New (untrusted) header timestamp cannot occur *after* next
                // consensus state's height
                let next_cs: ConsensusStateType = next_cs.try_into().map_err(Into::into)?;

                if header.signed_header.header().time >= next_cs.timestamp() {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }
}
