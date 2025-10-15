use std::io::Result;

fn main() -> Result<()> {
    // Configure prost-build
    let mut config = prost_build::Config::new();

    // Note: prost already derives Eq for enums via ::prost::Enumeration
    // Only add Eq to message types
    config.type_attribute(".gmp.GMPAcknowledgement", "#[derive(Eq)]");
    config.type_attribute(".gmp.GMPPacketData", "#[derive(Eq)]");
    config.type_attribute(".solana.SolanaInstruction", "#[derive(Eq)]");
    config.type_attribute(".solana.SolanaAccountMeta", "#[derive(Eq)]");

    // Compile proto files
    config.compile_protos(
        &[
            "../../../../proto/gmp/gmp.proto",
            "../../../../proto/solana/solana_instruction.proto",
        ],
        &["../../../../proto"],
    )?;

    println!("cargo:rerun-if-changed=../../../../proto/gmp/gmp.proto");
    println!("cargo:rerun-if-changed=../../../../proto/solana/solana_instruction.proto");

    Ok(())
}
