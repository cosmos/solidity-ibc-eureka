//! Programs for `sp1-ics07-tendermint`.

use sp1_sdk::{Prover, ProverClient, SP1VerifyingKey};

/// Trait for SP1 ICS07 Tendermint programs.
pub trait SP1Program {
    /// Get the ELF bytes for the program.
    fn elf(&self) -> &[u8];

    /// Get the verifying key for the program using [`MockProver`].
    #[must_use]
    fn get_vkey(&self) -> SP1VerifyingKey {
        let mock_prover = ProverClient::builder().mock().build();
        let (_, vkey) = mock_prover.setup(self.elf());
        vkey
    }
}

/// SP1 ICS07 Tendermint programs.
pub struct SP1ICS07TendermintPrograms {
    /// The update client program.
    pub update_client: UpdateClientProgram,
    /// The membership program.
    pub membership: MembershipProgram,
    /// The update client and membership program.
    pub update_client_and_membership: UpdateClientAndMembershipProgram,
    /// The misbehaviour program.
    pub misbehaviour: MisbehaviourProgram,
}

/// SP1 ICS07 Tendermint update client program.
pub struct UpdateClientProgram(Vec<u8>);

/// SP1 ICS07 Tendermint verify (non)membership program.
pub struct MembershipProgram(Vec<u8>);

/// SP1 ICS07 Tendermint update client and verify (non)membership program.
pub struct UpdateClientAndMembershipProgram(Vec<u8>);

/// SP1 ICS07 Tendermint misbehaviour program.
pub struct MisbehaviourProgram(Vec<u8>);

impl UpdateClientProgram {
    /// Create a new [`UpdateClientProgram`] from the given ELF bytes.
    #[must_use]
    pub const fn new(elf: Vec<u8>) -> Self {
        Self(elf)
    }
}

impl MembershipProgram {
    /// Create a new [`MembershipProgram`] from the given ELF bytes.
    #[must_use]
    pub const fn new(elf: Vec<u8>) -> Self {
        Self(elf)
    }
}

impl UpdateClientAndMembershipProgram {
    /// Create a new [`UpdateClientAndMembershipProgram`] from the given ELF bytes.
    #[must_use]
    pub const fn new(elf: Vec<u8>) -> Self {
        Self(elf)
    }
}

impl MisbehaviourProgram {
    /// Create a new [`MisbehaviourProgram`] from the given ELF bytes.
    #[must_use]
    pub const fn new(elf: Vec<u8>) -> Self {
        Self(elf)
    }
}

impl SP1Program for UpdateClientProgram {
    fn elf(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl SP1Program for MembershipProgram {
    fn elf(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl SP1Program for UpdateClientAndMembershipProgram {
    fn elf(&self) -> &[u8] {
        self.0.as_slice()
    }
}

impl SP1Program for MisbehaviourProgram {
    fn elf(&self) -> &[u8] {
        self.0.as_slice()
    }
}
