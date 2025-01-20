//! Prover for SP1 ICS07 Tendermint programs.

use crate::programs::{
    MembershipProgram, MisbehaviourProgram, SP1Program, UpdateClientAndMembershipProgram,
    UpdateClientProgram,
};
use ibc_client_tendermint_types::{Header, Misbehaviour};
use ibc_core_commitment_types::merkle::MerkleProof;
use ibc_eureka_solidity_types::sp1_ics07::{
    IICS07TendermintMsgs::{ClientState as SolClientState, ConsensusState as SolConsensusState},
    ISP1Msgs::SupportedZkAlgorithm,
};
use ibc_proto::Protobuf;
use sp1_prover::components::SP1ProverComponents;
use sp1_sdk::{
    Prover, SP1ProofMode, SP1ProofWithPublicValues, SP1ProvingKey, SP1Stdin, SP1VerifyingKey,
};

/// A prover for for [`SP1Program`] programs.
#[allow(clippy::module_name_repetitions)]
pub struct SP1ICS07TendermintProver<'a, T, C>
where
    T: SP1Program,
    C: SP1ProverComponents,
{
    /// [`sp1_sdk::ProverClient`] for generating proofs.
    pub prover_client: &'a dyn Prover<C>,
    /// The proving key.
    pub pkey: SP1ProvingKey,
    /// The verifying key.
    pub vkey: SP1VerifyingKey,
    /// The proof type.
    pub proof_type: SupportedProofType,
    _phantom: std::marker::PhantomData<T>,
    _phantom2: std::marker::PhantomData<C>,
}

/// The supported proof types.
#[derive(Clone, Debug, Copy)]
pub enum SupportedProofType {
    /// Groth16 proof.
    Groth16,
    /// Plonk proof.
    Plonk,
}

impl<'a, T, C> SP1ICS07TendermintProver<'a, T, C>
where
    T: SP1Program,
    C: SP1ProverComponents,
{
    /// Create a new prover.
    #[must_use]
    #[tracing::instrument(skip_all)]
    pub fn new(proof_type: SupportedProofType, prover_client: &'a dyn Prover<C>) -> Self {
        tracing::info!("Initializing SP1 ProverClient...");
        let (pkey, vkey) = prover_client.setup(T::ELF);
        tracing::info!("SP1 ProverClient initialized");
        Self {
            prover_client,
            pkey,
            vkey,
            proof_type,
            _phantom: std::marker::PhantomData,
            _phantom2: std::marker::PhantomData,
        }
    }

    /// Prove the given input.
    /// # Panics
    /// If the proof cannot be generated or validated.
    #[must_use]
    pub fn prove(&self, stdin: &SP1Stdin) -> SP1ProofWithPublicValues {
        // Generate the proof. Depending on SP1_PROVER env variable, this may be a mock, local or
        // network proof.
        let proof = self
            .prover_client
            .prove(&self.pkey, stdin, self.proof_type.to_proof_mode())
            .expect("proving failed");

        self.prover_client
            .verify(&proof, &self.vkey)
            .expect("verification failed");

        proof
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
        time: u64,
    ) -> SP1ProofWithPublicValues {
        // Encode the inputs into our program.
        let encoded_1 = bincode::serialize(client_state).unwrap();
        let encoded_2 = bincode::serialize(&trusted_consensus_state).unwrap();
        let encoded_3 = serde_cbor::to_vec(proposed_header).unwrap();
        let encoded_4 = time.to_le_bytes().into();
        // TODO: find an encoding that works for all the structs above.

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
        kv_proofs: Vec<(Vec<Vec<u8>>, Vec<u8>, MerkleProof)>,
    ) -> SP1ProofWithPublicValues {
        assert!(!kv_proofs.is_empty(), "No key-value pairs to prove");
        let len = u8::try_from(kv_proofs.len()).expect("too many key-value pairs");

        let mut stdin = SP1Stdin::new();
        stdin.write_slice(commitment_root);
        stdin.write_vec(vec![len]);
        for (path, value, proof) in kv_proofs {
            stdin.write_vec(bincode::serialize(&path).unwrap());
            stdin.write_vec(value);
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
        time: u64,
        kv_proofs: Vec<(Vec<Vec<u8>>, Vec<u8>, MerkleProof)>,
    ) -> SP1ProofWithPublicValues {
        assert!(!kv_proofs.is_empty(), "No key-value pairs to prove");
        let len = u8::try_from(kv_proofs.len()).expect("too many key-value pairs");
        // Encode the inputs into our program.
        let encoded_1 = bincode::serialize(client_state).unwrap();
        let encoded_2 = bincode::serialize(&trusted_consensus_state).unwrap();
        // NOTE: The Header struct is not deserializable by bincode, so we use CBOR instead.
        let encoded_3 = serde_cbor::to_vec(proposed_header).unwrap();
        let encoded_4 = time.to_le_bytes().into();
        // TODO: find an encoding that works for all the structs above.

        // Write the encoded light blocks to stdin.
        let mut stdin = SP1Stdin::new();
        stdin.write_vec(encoded_1);
        stdin.write_vec(encoded_2);
        stdin.write_vec(encoded_3);
        stdin.write_vec(encoded_4);
        stdin.write_vec(vec![len]);
        for (path, value, proof) in kv_proofs {
            stdin.write_vec(bincode::serialize(&path).unwrap());
            stdin.write_vec(value);
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
        time: u64,
    ) -> SP1ProofWithPublicValues {
        let encoded_1 = bincode::serialize(client_state).unwrap();
        let encoded_2 = serde_cbor::to_vec(misbehaviour).unwrap();
        let encoded_3 = bincode::serialize(trusted_consensus_state_1).unwrap();
        let encoded_4 = bincode::serialize(trusted_consensus_state_2).unwrap();
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

impl From<SupportedProofType> for SupportedZkAlgorithm {
    fn from(proof_type: SupportedProofType) -> Self {
        match proof_type {
            SupportedProofType::Groth16 => Self::from(0),
            SupportedProofType::Plonk => Self::from(1),
        }
    }
}

impl TryFrom<u8> for SupportedProofType {
    type Error = String;

    fn try_from(n: u8) -> Result<Self, Self::Error> {
        match n {
            0 => Ok(Self::Groth16),
            1 => Ok(Self::Plonk),
            n => Err(format!("Unsupported proof type: {n}")),
        }
    }
}

impl SupportedProofType {
    /// Convert the proof type to a [`SP1ProofMode`].
    #[must_use]
    pub const fn to_proof_mode(&self) -> SP1ProofMode {
        match self {
            Self::Groth16 => SP1ProofMode::Groth16,
            Self::Plonk => SP1ProofMode::Plonk,
        }
    }
}
