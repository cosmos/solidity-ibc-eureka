//! Runner for generating `misbehaviour` fixtures

use crate::{
    cli::command::{fixtures::MisbehaviourCmd, OutputPath},
    runners::genesis::SP1ICS07TendermintGenesis,
};
use alloy::sol_types::SolValue;
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::{ClientState, ConsensusState},
    IMisbehaviourMsgs::MsgSubmitMisbehaviour,
    ISP1Msgs::SP1Proof,
};
use ibc_eureka_utils::rpc::TendermintRpcExt;
use ibc_proto::ibc::lightclients::tendermint::v1::Misbehaviour as RawMisbehaviour;
use serde::{Deserialize, Serialize};
use sp1_ics07_tendermint_prover::{
    programs::{
        MembershipProgram, MisbehaviourProgram, UpdateClientAndMembershipProgram,
        UpdateClientProgram,
    },
    prover::{SP1ICS07TendermintProver, Sp1Prover},
};
use sp1_sdk::{network::FulfillmentStrategy, HashableKey, ProverClient};
use std::path::PathBuf;
use tendermint_rpc::HttpClient;

/// The fixture data to be used in [`SP1ICS07SubmitMisbehaviourFixture`] tests.
#[serde_with::serde_as]
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SP1ICS07SubmitMisbehaviourFixture {
    /// The genesis data.
    #[serde(flatten)]
    genesis: SP1ICS07TendermintGenesis,

    /// The encoded submit misbehaviour client message.
    #[serde_as(as = "serde_with::hex::Hex")]
    submit_msg: Vec<u8>,
}

/// Writes the proof data for misbehaviour to the given fixture path.
#[allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]
pub async fn run(args: MisbehaviourCmd) -> anyhow::Result<()> {
    let misbehaviour: RawMisbehaviour =
        serde_json::from_slice(&std::fs::read(args.misbehaviour_json_path)?)?;

    let update_client_elf = std::fs::read(args.elf_paths.update_client_path)?;
    let membership_elf = std::fs::read(args.elf_paths.membership_path)?;
    let misbehaviour_elf = std::fs::read(args.elf_paths.misbehaviour_path)?;
    let uc_and_membership_elf = std::fs::read(args.elf_paths.uc_and_membership_path)?;
    let update_client_program = UpdateClientProgram::new(update_client_elf);
    let membership_program = MembershipProgram::new(membership_elf);
    let misbehaviour_program = MisbehaviourProgram::new(misbehaviour_elf);
    let uc_and_membership_program = UpdateClientAndMembershipProgram::new(uc_and_membership_elf);

    let tm_rpc_client = HttpClient::from_env();
    let sp1_prover = if args.sp1.private_cluster {
        Sp1Prover::Network(
            ProverClient::builder().network().build().await,
            FulfillmentStrategy::Reserved,
        )
    } else {
        Sp1Prover::Env(ProverClient::from_env().await)
    };

    #[allow(clippy::cast_possible_truncation)]
    let height_1 = misbehaviour
        .header_1
        .as_ref()
        .unwrap()
        .trusted_height
        .unwrap()
        .revision_height;
    #[allow(clippy::cast_possible_truncation)]
    let height_2 = misbehaviour
        .header_2
        .as_ref()
        .unwrap()
        .trusted_height
        .unwrap()
        .revision_height;
    let trusted_light_block_1 = tm_rpc_client.get_light_block(Some(height_1)).await?;
    let trusted_light_block_2 = tm_rpc_client.get_light_block(Some(height_2)).await?;

    let genesis_1 = SP1ICS07TendermintGenesis::from_env(
        &trusted_light_block_1,
        args.trust_options.trusting_period,
        args.trust_options.trust_level,
        args.sp1.proof_type,
        &update_client_program,
        &membership_program,
        &uc_and_membership_program,
        &misbehaviour_program,
    )
    .await?;
    let genesis_2 = SP1ICS07TendermintGenesis::from_env(
        &trusted_light_block_2,
        args.trust_options.trusting_period,
        args.trust_options.trust_level,
        args.sp1.proof_type,
        &update_client_program,
        &membership_program,
        &uc_and_membership_program,
        &misbehaviour_program,
    )
    .await?;
    let trusted_consensus_state_1 = ConsensusState::abi_decode(&genesis_1.trusted_consensus_state)?;
    let trusted_consensus_state_2 = ConsensusState::abi_decode(&genesis_2.trusted_consensus_state)?;
    let trusted_client_state_2 = ClientState::abi_decode(&genesis_2.trusted_client_state)?;
    let verify_misbehaviour_prover =
        SP1ICS07TendermintProver::new(args.sp1.proof_type, &sp1_prover, &misbehaviour_program)
            .await;
    let now_since_unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    let proof_data = verify_misbehaviour_prover
        .generate_proof(
            &trusted_client_state_2,
            &misbehaviour,
            &trusted_consensus_state_1,
            &trusted_consensus_state_2,
            now_since_unix.as_nanos(),
        )
        .await;
    let submit_msg = MsgSubmitMisbehaviour {
        sp1Proof: SP1Proof::new(
            &verify_misbehaviour_prover.vkey.bytes32(),
            proof_data.bytes(),
            proof_data.public_values.to_vec(),
        ),
    };
    let fixture = SP1ICS07SubmitMisbehaviourFixture {
        genesis: genesis_2,
        submit_msg: submit_msg.abi_encode(),
    };

    match args.output_path {
        OutputPath::File(path) => {
            std::fs::write(PathBuf::from(path), serde_json::to_string_pretty(&fixture)?)?;
        }
        OutputPath::Stdout => {
            println!("{}", serde_json::to_string_pretty(&fixture)?);
        }
    }

    Ok(())
}
