//! Contains the runner for the genesis command.

use crate::cli::command::{genesis::Args, OutputPath};
use alloy::sol_types::SolValue;
use ibc_eureka_solidity_types::msgs::IICS07TendermintMsgs::{
    ConsensusState as SolConsensusState, SupportedZkAlgorithm,
};
use ibc_eureka_utils::{light_block::LightBlockExt, rpc::TendermintRpcExt};
use sp1_ics07_tendermint_prover::programs::{
    MembershipProgram, MisbehaviourProgram, SP1Program, UpdateClientAndMembershipProgram,
    UpdateClientProgram,
};
use sp1_sdk::{utils::setup_logger, HashableKey};
use std::path::PathBuf;
use tendermint_light_client_verifier::types::{LightBlock, TrustThreshold};
use tendermint_rpc::HttpClient;

/// The genesis data for the SP1 ICS07 Tendermint contract.
#[serde_with::serde_as]
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::module_name_repetitions)]
pub struct SP1ICS07TendermintGenesis {
    /// The encoded trusted client state.
    #[serde_as(as = "serde_with::hex::Hex")]
    pub trusted_client_state: Vec<u8>,
    /// The encoded trusted consensus state.
    #[serde_as(as = "serde_with::hex::Hex")]
    pub trusted_consensus_state: Vec<u8>,
    /// The encoded key for [`UpdateClientProgram`].
    update_client_vkey: String,
    /// The encoded key for [`MembershipProgram`].
    membership_vkey: String,
    /// The encoded key for [`UpdateClientAndMembershipProgram`].
    uc_and_membership_vkey: String,
    /// The encoded key for [`MisbehaviourProgram`].
    misbehaviour_vkey: String,
}

impl SP1ICS07TendermintGenesis {
    /// Creates a new genesis instance by reading the environment variables
    /// and making the necessary RPC calls.
    #[allow(clippy::missing_errors_doc, clippy::too_many_arguments)]
    pub async fn from_env(
        trusted_light_block: &LightBlock,
        trusting_period: Option<u32>,
        trust_level: TrustThreshold,
        proof_type: SupportedZkAlgorithm,
        update_client: &UpdateClientProgram,
        membership: &MembershipProgram,
        uc_and_membership: &UpdateClientAndMembershipProgram,
        misbehaviour: &MisbehaviourProgram,
    ) -> anyhow::Result<Self> {
        setup_logger();
        if dotenv::dotenv().is_err() {
            tracing::warn!("No .env file found");
        }

        let tm_rpc_client = HttpClient::from_env();

        let unbonding_period = tm_rpc_client
            .sdk_staking_params()
            .await?
            .unbonding_time
            .ok_or_else(|| anyhow::anyhow!("No unbonding time found"))?
            .seconds
            .try_into()?;

        // Defaults to the recommended TrustingPeriod: 2/3 of the UnbondingPeriod
        let trusting_period = trusting_period.unwrap_or(2 * (unbonding_period / 3));
        if trusting_period > unbonding_period {
            return Err(anyhow::anyhow!(
                "Trusting period cannot be greater than unbonding period"
            ));
        }

        let trusted_client_state = trusted_light_block.to_sol_client_state(
            trust_level.try_into()?,
            unbonding_period,
            trusting_period,
            proof_type,
        )?;
        let trusted_consensus_state = trusted_light_block.to_consensus_state();

        Ok(Self {
            trusted_consensus_state: SolConsensusState::from(trusted_consensus_state).abi_encode(),
            trusted_client_state: trusted_client_state.abi_encode(),
            update_client_vkey: update_client.get_vkey().bytes32(),
            membership_vkey: membership.get_vkey().bytes32(),
            uc_and_membership_vkey: uc_and_membership.get_vkey().bytes32(),
            misbehaviour_vkey: misbehaviour.get_vkey().bytes32(),
        })
    }
}

/// Creates the `genesis.json` file for the `SP1ICS07Tendermint` contract.
#[allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
pub async fn run(args: Args) -> anyhow::Result<()> {
    let tm_rpc_client = HttpClient::from_env();

    let trusted_light_block = tm_rpc_client.get_light_block(args.trusted_block).await?;
    if args.trusted_block.is_none() {
        tracing::info!(
            "Latest block height: {}",
            trusted_light_block.height().value()
        );
    }

    let update_client_elf = std::fs::read(args.elf_paths.update_client_path)?;
    let membership_elf = std::fs::read(args.elf_paths.membership_path)?;
    let uc_and_membership_elf = std::fs::read(args.elf_paths.uc_and_membership_path)?;
    let misbehaviour_elf = std::fs::read(args.elf_paths.misbehaviour_path)?;
    let update_client_program = UpdateClientProgram::new(update_client_elf);
    let membership_program = MembershipProgram::new(membership_elf);
    let uc_and_membership_program = UpdateClientAndMembershipProgram::new(uc_and_membership_elf);
    let misbehaviour_program = MisbehaviourProgram::new(misbehaviour_elf);

    let genesis = SP1ICS07TendermintGenesis::from_env(
        &trusted_light_block,
        args.trust_options.trusting_period,
        args.trust_options.trust_level,
        args.proof_type,
        &update_client_program,
        &membership_program,
        &uc_and_membership_program,
        &misbehaviour_program,
    )
    .await?;

    match args.output_path {
        OutputPath::File(path) => {
            // Save the proof data to the file path.
            std::fs::write(PathBuf::from(path), serde_json::to_string_pretty(&genesis)?)?;
        }
        OutputPath::Stdout => {
            println!("{}", serde_json::to_string_pretty(&genesis)?);
        }
    }

    Ok(())
}
