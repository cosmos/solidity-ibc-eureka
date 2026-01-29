//! Build script to generate compile-time constants.
//!
//! Generates:
//! - EVM function selectors (keccak256 of Solidity signatures)
//! - Anchor discriminators (sha256 of instruction names)
//! - IBC commitment constants

use sha2::{Digest, Sha256};
use std::env;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// EVM function signatures: `(constant_name, solidity_signature)`
const EVM_SELECTORS: &[(&str, &str)] = &[("IFT_MINT_SELECTOR", "iftMint(address,uint256)")];

/// Anchor discriminators: `(constant_name, instruction_name)`
const ANCHOR_DISCRIMINATORS: &[(&str, &str)] = &[("IFT_MINT_DISCRIMINATOR", "global:ift_mint")];

/// IBC version byte for commitment computation
const IBC_VERSION: u8 = 0x02;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("evm_selectors.rs");
    let mut f = File::create(&dest_path).unwrap();

    writeln!(f, "// Auto-generated constants - DO NOT EDIT").unwrap();
    writeln!(f).unwrap();

    // Generate EVM selectors
    for (name, signature) in EVM_SELECTORS {
        let hash = solana_keccak_hasher::hash(signature.as_bytes());
        let selector = &hash.to_bytes()[..4];
        writeln!(f, "/// `keccak256(\"{signature}\")[0..4]`").unwrap();
        writeln!(
            f,
            "pub const {name}: [u8; 4] = [0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}];",
            selector[0], selector[1], selector[2], selector[3]
        )
        .unwrap();
        writeln!(f).unwrap();
    }

    // Generate Anchor discriminators
    for (name, instruction) in ANCHOR_DISCRIMINATORS {
        let mut hasher = Sha256::new();
        hasher.update(instruction.as_bytes());
        let hash = hasher.finalize();
        let disc = &hash[..8];
        writeln!(f, "/// `sha256(\"{instruction}\")[0..8]`").unwrap();
        writeln!(
            f,
            "pub const {name}: [u8; 8] = [0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}];",
            disc[0], disc[1], disc[2], disc[3], disc[4], disc[5], disc[6], disc[7]
        )
        .unwrap();
        writeln!(f).unwrap();
    }

    // Generate ERROR_ACK_COMMITMENT constant
    // UNIVERSAL_ERROR_ACK = sha256("UNIVERSAL_ERROR_ACKNOWLEDGEMENT")
    // ERROR_ACK_COMMITMENT = sha256(0x02 || sha256(UNIVERSAL_ERROR_ACK))
    let universal_error_ack = Sha256::digest(b"UNIVERSAL_ERROR_ACKNOWLEDGEMENT");
    let inner_hash = Sha256::digest(universal_error_ack);
    let mut commitment_input = vec![IBC_VERSION];
    commitment_input.extend_from_slice(&inner_hash);
    let error_ack_commitment = Sha256::digest(&commitment_input);

    writeln!(
        f,
        "/// IBC commitment for the universal error acknowledgement."
    )
    .unwrap();
    writeln!(
        f,
        "/// Computed as: `sha256(0x02 || sha256(sha256(\"UNIVERSAL_ERROR_ACKNOWLEDGEMENT\")))`"
    )
    .unwrap();
    write!(f, "pub const ERROR_ACK_COMMITMENT: [u8; 32] = [").unwrap();
    for (i, byte) in error_ack_commitment.iter().enumerate() {
        if i > 0 {
            write!(f, ", ").unwrap();
        }
        write!(f, "0x{byte:02x}").unwrap();
    }
    writeln!(f, "];").unwrap();
    writeln!(f).unwrap();

    println!("cargo:rerun-if-changed=build.rs");
}
