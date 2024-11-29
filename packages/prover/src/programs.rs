//! Programs for `sp1-ics07-tendermint`.

use sp1_sdk::{MockProver, Prover, SP1VerifyingKey};

/// Trait for SP1 ICS07 Tendermint programs.
pub trait SP1Program {
    /// The ELF file for the program.
    const ELF: &'static [u8];

    /// Get the verifying key for the program using [`MockProver`].
    #[must_use]
    fn get_vkey() -> SP1VerifyingKey {
        let mock_prover = MockProver::new();
        let (_, vkey) = mock_prover.setup(Self::ELF);
        vkey
    }
}

/// SP1 ICS07 Tendermint update client program.
pub struct UpdateClientProgram;

/// SP1 ICS07 Tendermint verify (non)membership program.
pub struct MembershipProgram;

/// SP1 ICS07 Tendermint update client and verify (non)membership program.
pub struct UpdateClientAndMembershipProgram;

/// SP1 ICS07 Tendermint misbehaviour program.
pub struct MisbehaviourProgram;

impl SP1Program for UpdateClientProgram {
    const ELF: &'static [u8] =
        include_bytes!("../../../elf/update-client-riscv32im-succinct-zkvm-elf");
}

impl SP1Program for MembershipProgram {
    const ELF: &'static [u8] =
        include_bytes!("../../../elf/membership-riscv32im-succinct-zkvm-elf");
}

impl SP1Program for UpdateClientAndMembershipProgram {
    const ELF: &'static [u8] =
        include_bytes!("../../../elf/uc-and-membership-riscv32im-succinct-zkvm-elf");
}

impl SP1Program for MisbehaviourProgram {
    const ELF: &'static [u8] =
        include_bytes!("../../../elf/misbehaviour-riscv32im-succinct-zkvm-elf");
}
