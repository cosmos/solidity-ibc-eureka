use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let descriptor_path = out_dir.join("ibc_attestor_descriptor.bin");
    tonic_build::configure()
        .file_descriptor_set_path(&descriptor_path)
        .use_arc_self(true)
        .build_server(true)
        .compile_protos(
            &["../../proto/ibc_attestor/ibc_attestor.proto"],
            &["../../proto/ibc_attestor"],
        )?;
    Ok(())
}
