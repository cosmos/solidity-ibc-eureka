//! Prover for SP1 ICS07 Tendermint programs.

use crate::programs::{
    MembershipProgram, MisbehaviourProgram, SP1Program, UpdateClientAndMembershipProgram,
    UpdateClientProgram,
};
use alloy_sol_types::SolValue;
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_eureka_solidity_types::msgs::{
    IICS07TendermintMsgs::{ClientState as SolClientState, ConsensusState as SolConsensusState},
    IMembershipMsgs::KVPair,
};
use ibc_proto::{
    ibc::lightclients::tendermint::v1::{Header, Misbehaviour},
    Protobuf,
};
use prost::Message;
use sp1_prover::components::SP1ProverComponents;
use sp1_sdk::{
    network::FulfillmentStrategy, NetworkProver, Prover, SP1ProofMode, SP1ProofWithPublicValues,
    SP1ProvingKey, SP1Stdin, SP1VerifyingKey,
};

// Re-export the supported zk algorithms.
pub use ibc_eureka_solidity_types::msgs::IICS07TendermintMsgs::SupportedZkAlgorithm;

/// A prover for for [`SP1Program`] programs.
#[allow(clippy::module_name_repetitions)]
pub struct SP1ICS07TendermintProver<'a, T, C>
where
    T: SP1Program,
    C: SP1ProverComponents,
{
    /// [`sp1_sdk::ProverClient`] for generating proofs.
    pub prover_client: &'a Sp1Prover<C>,
    /// The proving key.
    pub pkey: SP1ProvingKey,
    /// The verifying key.
    pub vkey: SP1VerifyingKey,
    /// The proof type.
    pub proof_type: SupportedZkAlgorithm,
    /// The program type.
    pub program: &'a T,
}

/// The SP1 prover used by the [`TxBuilder`].
#[allow(clippy::module_name_repetitions)]
pub enum Sp1Prover<C>
where
    C: SP1ProverComponents,
{
    /// All default public cluster provers.
    PublicCluster(Box<dyn Prover<C>>),
    /// Private clusters can only be a [`NetworkProver`].
    PrivateCluster(Box<NetworkProver>),
}

impl<C: SP1ProverComponents> Sp1Prover<C> {
    /// Create a new [`Sp1Prover::PublicCluster`] from any type that implements [`Prover`].
    pub fn new_public_cluster<T>(prover: T) -> Self
    where
        T: Prover<C> + 'static,
    {
        Self::PublicCluster(Box::new(prover))
    }

    /// Create a new [`Sp1Prover::PrivateCluster`] from a [`NetworkProver`].
    pub fn new_private_cluster(prover: NetworkProver) -> Self {
        Self::PrivateCluster(Box::new(prover))
    }

    /// Calls the underlying prover's `setup` method.
    #[must_use]
    pub fn setup(&self, elf: &[u8]) -> (SP1ProvingKey, SP1VerifyingKey) {
        match self {
            Self::PublicCluster(prover) => prover.setup(elf),
            Self::PrivateCluster(prover) => prover.setup(elf),
        }
    }
}

impl<'a, T, C> SP1ICS07TendermintProver<'a, T, C>
where
    T: SP1Program,
    C: SP1ProverComponents,
{
    /// Create a new prover.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn new(
        proof_type: SupportedZkAlgorithm,
        prover_client: &'a Sp1Prover<C>,
        program: &'a T,
    ) -> Self {
        tracing::info!("Initializing SP1 ProverClient...");
        let (pkey, vkey) = prover_client.setup(program.elf());
        tracing::info!("SP1 ProverClient initialized");
        Self {
            prover_client,
            pkey,
            vkey,
            proof_type,
            program,
        }
    }

    /// Prove the given input.
    /// # Panics
    /// If the proof cannot be generated or validated.
    #[must_use]
    pub fn prove(&self, stdin: &SP1Stdin) -> SP1ProofWithPublicValues {
        // Generate the proof. Depending on SP1_PROVER env variable, this may be a mock, local or
        match self.prover_client {
            Sp1Prover::PublicCluster(ref prover) => prover
                .prove(
                    &self.pkey,
                    stdin,
                    match self.proof_type {
                        SupportedZkAlgorithm::Groth16 => SP1ProofMode::Groth16,
                        SupportedZkAlgorithm::Plonk => SP1ProofMode::Plonk,
                        SupportedZkAlgorithm::__Invalid => panic!("unsupported zk algorithm"),
                    },
                )
                .expect("proving failed"),
            Sp1Prover::PrivateCluster(ref prover) => match self.proof_type {
                SupportedZkAlgorithm::Groth16 => prover.prove(&self.pkey, stdin).groth16(),
                SupportedZkAlgorithm::Plonk => prover.prove(&self.pkey, stdin).plonk(),
                SupportedZkAlgorithm::__Invalid => panic!("unsupported zk algorithm"),
            }
            .strategy(FulfillmentStrategy::Reserved)
            .run()
            .expect("proving failed"),
        }
    }
}

impl<C> SP1ICS07TendermintProver<'_, UpdateClientProgram, C>
where
    C: SP1ProverComponents,
{
    /// Generate a proof of an update from `trusted_consensus_state` to a proposed header.
    ///
    /// # Panics
    /// Panics if the inputs cannot be encoded, the proof cannot be generated or the proof is
    /// invalid.
    #[must_use]
    pub fn generate_proof(
        &self,
        client_state: &SolClientState,
        trusted_consensus_state: &SolConsensusState,
        proposed_header: &Header,
        time: u128,
    ) -> SP1ProofWithPublicValues {
        // Encode the inputs into our program.
        let encoded_1 = client_state.abi_encode();
        let encoded_2 = trusted_consensus_state.abi_encode();
        let encoded_3 = proposed_header.encode_to_vec();
        let encoded_4 = time.to_le_bytes().into();

        // Write the encoded light blocks to stdin.
        let mut stdin = SP1Stdin::new();
        stdin.write_vec(encoded_1);
        stdin.write_vec(encoded_2);
        stdin.write_vec(encoded_3);
        stdin.write_vec(encoded_4);

        self.prove(&stdin)
    }
}

impl<C> SP1ICS07TendermintProver<'_, MembershipProgram, C>
where
    C: SP1ProverComponents,
{
    /// Generate a proof of verify (non)membership for multiple key-value pairs.
    ///
    /// # Panics
    /// Panics if the proof cannot be generated or the proof is invalid.
    #[must_use]
    pub fn generate_proof(
        &self,
        commitment_root: &[u8],
        kv_proofs: Vec<(KVPair, MerkleProof)>,
    ) -> SP1ProofWithPublicValues {
        assert!(!kv_proofs.is_empty(), "No key-value pairs to prove");
        let len = u16::try_from(kv_proofs.len()).expect("too many key-value pairs");

        let mut stdin = SP1Stdin::new();
        stdin.write_slice(commitment_root);
        stdin.write_slice(&len.to_le_bytes());
        for (kv_pair, proof) in kv_proofs {
            stdin.write_vec(kv_pair.abi_encode());
            stdin.write_vec(proof.encode_vec());
        }

        self.prove(&stdin)
    }
}

impl<C> SP1ICS07TendermintProver<'_, UpdateClientAndMembershipProgram, C>
where
    C: SP1ProverComponents,
{
    /// Generate a proof of an update from `trusted_consensus_state` to a proposed header and
    /// verify (non)membership for multiple key-value pairs on the commitment root of
    /// `proposed_header`.
    ///
    /// # Panics
    /// Panics if the inputs cannot be encoded, the proof cannot be generated or the proof is
    /// invalid.
    #[must_use]
    pub fn generate_proof(
        &self,
        client_state: &SolClientState,
        trusted_consensus_state: &SolConsensusState,
        proposed_header: &Header,
        time: u128,
        kv_proofs: Vec<(KVPair, MerkleProof)>,
    ) -> SP1ProofWithPublicValues {
        assert!(!kv_proofs.is_empty(), "No key-value pairs to prove");
        let len = u16::try_from(kv_proofs.len()).expect("too many key-value pairs");
        // Encode the inputs into our program.
        let encoded_1 = client_state.abi_encode();
        let encoded_2 = trusted_consensus_state.abi_encode();
        let encoded_3 = proposed_header.encode_to_vec();
        let encoded_4 = time.to_le_bytes().into();

        // Write the encoded light blocks to stdin.
        let mut stdin = SP1Stdin::new();
        stdin.write_vec(encoded_1);
        stdin.write_vec(encoded_2);
        stdin.write_vec(encoded_3);
        stdin.write_vec(encoded_4);
        stdin.write_slice(&len.to_le_bytes());
        for (kv_pair, proof) in kv_proofs {
            stdin.write_vec(kv_pair.abi_encode());
            stdin.write_vec(proof.encode_vec());
        }

        self.prove(&stdin)
    }
}

impl<C> SP1ICS07TendermintProver<'_, MisbehaviourProgram, C>
where
    C: SP1ProverComponents,
{
    /// Generate a proof of a misbehaviour.
    ///
    /// # Panics
    /// Panics if the proof cannot be generated or the proof is invalid.
    #[must_use]
    pub fn generate_proof(
        &self,
        client_state: &SolClientState,
        misbehaviour: &Misbehaviour,
        trusted_consensus_state_1: &SolConsensusState,
        trusted_consensus_state_2: &SolConsensusState,
        time: u128,
    ) -> SP1ProofWithPublicValues {
        let encoded_1 = client_state.abi_encode();
        let encoded_2 = misbehaviour.encode_to_vec();
        let encoded_3 = trusted_consensus_state_1.abi_encode();
        let encoded_4 = trusted_consensus_state_2.abi_encode();
        let encoded_5 = time.to_le_bytes().into();

        let mut stdin = SP1Stdin::new();
        stdin.write_vec(encoded_1);
        stdin.write_vec(encoded_2);
        stdin.write_vec(encoded_3);
        stdin.write_vec(encoded_4);
        stdin.write_vec(encoded_5);

        self.prove(&stdin)
    }
}

impl<C: SP1ProverComponents> From<Box<dyn Prover<C>>> for Sp1Prover<C> {
    fn from(prover: Box<dyn Prover<C>>) -> Self {
        Self::PublicCluster(prover)
    }
}

impl<C: SP1ProverComponents> From<NetworkProver> for Sp1Prover<C> {
    fn from(prover: NetworkProver) -> Self {
        Self::PrivateCluster(Box::new(prover))
    }
}
