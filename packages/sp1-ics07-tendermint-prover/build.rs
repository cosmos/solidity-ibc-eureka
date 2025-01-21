use sp1_helper::{build_program_with_args, BuildArgs};

// Build script to build the programs if they change.
// Requires SP1 toolchain to be installed.
fn main() {
    // Build the update-client program.
    build_program_with_args(
        "../../programs/sp1-programs/update-client",
        BuildArgs {
            elf_name: Some("update-client-riscv32im-succinct-zkvm-elf".to_string()),
            locked: true,
            output_directory: Some("../../elf".to_string()),
            ..Default::default()
        },
    );
    // Build the membership program.
    build_program_with_args(
        "../../programs/sp1-programs/membership",
        BuildArgs {
            elf_name: Some("membership-riscv32im-succinct-zkvm-elf".to_string()),
            locked: true,
            output_directory: Some("../../elf".to_string()),
            ..Default::default()
        },
    );
    // Build the uc-and-membership program.
    build_program_with_args(
        "../../programs/sp1-programs/uc-and-membership",
        BuildArgs {
            elf_name: Some("uc-and-membership-riscv32im-succinct-zkvm-elf".to_string()),
            locked: true,
            output_directory: Some("../../elf".to_string()),
            ..Default::default()
        },
    );
    // Build the misbehaviour program.
    build_program_with_args(
        "../../programs/sp1-programs/misbehaviour",
        BuildArgs {
            elf_name: Some("misbehaviour-riscv32im-succinct-zkvm-elf".to_string()),
            locked: true,
            output_directory: Some("../../elf".to_string()),
            ..Default::default()
        },
    )
}
