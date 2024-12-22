use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run git command to get the current commit hash
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .expect("Failed to execute git command");

    let git_hash = String::from_utf8(output.stdout)
        .expect("Invalid UTF-8 sequence")
        .trim()
        .to_string();

    // Pass the git hash to the main program
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    tonic_build::configure()
        .build_server(true)
        .compile_protos(&["proto/relayer/relayer.proto"], &["proto/relayer"])?;
    Ok(())
}
