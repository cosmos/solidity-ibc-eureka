use std::io::Result;

fn main() -> Result<()> {
    // Configure prost-build
    let mut config = prost_build::Config::new();

    // Add Eq derive for message types
    config.type_attribute(".gmp.GMPAcknowledgement", "#[derive(Eq)]");
    config.type_attribute(".gmp.GMPPacketData", "#[derive(Eq)]");
    config.type_attribute(".solana.GMPSolanaPayload", "#[derive(Eq)]");
    config.type_attribute(".solana.SolanaAccountMeta", "#[derive(Eq)]");

    // Compile proto files
    config.compile_protos(
        &[
            "../../../../proto/gmp/gmp.proto",
            "../../../../proto/solana/gmp_solana_payload.proto",
        ],
        &["../../../../proto"],
    )?;

    println!("cargo:rerun-if-changed=../../../../proto/gmp/gmp.proto");
    println!("cargo:rerun-if-changed=../../../../proto/solana/gmp_solana_payload.proto");

    Ok(())
}
