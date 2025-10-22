{
  description = "Development environment for Solidity IBC Eureka";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    solc = {
      url = "github:hellwolf/solc.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    foundry.url = "github:shazow/foundry.nix/main";
    rust-overlay.url = "github:oxalica/rust-overlay";
    natlint.url = "github:srdtrk/natlint";
  };
  outputs = inputs: inputs.flake-utils.lib.eachSystem
    [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" ]
    (
      system:
      let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            (import inputs.rust-overlay)
            inputs.foundry.overlay
            inputs.solc.overlay
          ];
        };
        rust = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };
        solana-agave = pkgs.callPackage ./nix/agave.nix {
          inherit (pkgs) rust-bin anchor;
        };
        anchor-go = pkgs.callPackage ./nix/anchor-go.nix {};
      in
      {
        devShells = {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              openssl
              openssl.dev
              pkg-config
              foundry-bin
              go-ethereum
              solc_0_8_28
              (inputs.solc.mkDefault pkgs solc_0_8_28)
              bun
              just
              golangci-lint
              go
              jq
              parallel
              rust
              anchor
              anchor-go
              protobuf
              slither-analyzer
              buf
              protoc-gen-go
              protoc-gen-go-grpc
              quicktype
              inputs.natlint.packages.${system}.default
            ];
            NIX_LD_LIBRARY_PATH = with pkgs.buildPackages; lib.makeLibraryPath [
              stdenv.cc.cc
            ];
            shellHook = ''
              export RUST_SRC_PATH="${rust}/lib/rustlib/src/rust/library"
              if [ -z "$(which cargo-prove)" ]; then
                echo "SP1 toolchain is not installed. This is recommended to generate risc-v elfs. To install, please follow the instructions at"
                echo "https://docs.succinct.xyz/docs/sp1/getting-started/install"
              fi
            '';
          };
          solana = pkgs.mkShell {
            buildInputs = with pkgs; [
              gawk
              openssl
              openssl.dev
              pkg-config
              solana-agave
              anchor-go
              protobuf
              buf
              protoc-gen-go
              protoc-gen-go-grpc
              just
              rust
              golangci-lint
              go
              gopls
              gofumpt
            ];
            shellHook = ''
              export RUST_SRC_PATH="${rust}/lib/rustlib/src/rust/library"
              export PATH="${solana-agave}/bin:$PATH"
              echo ""
              echo "Solana development shell activated"
              echo ""
              echo "Available commands:"
              echo "  solana      - Solana CLI tool"
              echo "  anchor-nix  - Anchor wrapper for Nix environments"
              echo ""
              echo "Use 'anchor-nix' for optimized builds:"
              echo "  anchor-nix build                - Build with Solana toolchain + generate IDL with nightly"
              echo "  anchor-nix test                 - Build and run anchor client tests"
              echo "  anchor-nix unit-test [options]  - Build program then run cargo test"
              echo ""

              # WORKAROUND: Fix Darwin SDK conflicts (Oct 2025)
              # nixpkgs unstable has mismatched Apple SDK versions:
              # - clang-wrapper uses SDK 11.3 (sets DEVELOPER_DIR)
              # - libcxx uses SDK 15.5 (linked into binaries)
              # - rust toolchain tries to use SDK 12.3 (sets DEVELOPER_DIR_FOR_TARGET)
              # This causes "Multiple conflicting values defined for DEVELOPER_DIR" linker errors.
              # We unset the conflicting *_FOR_TARGET variables to force everything to use SDK 11.3.
              # Remove this workaround when nixpkgs fixes the SDK version mismatch upstream.
              if [[ "$OSTYPE" == "darwin"* ]]; then
                unset DEVELOPER_DIR_FOR_TARGET
                unset NIX_APPLE_SDK_VERSION_FOR_TARGET
                unset SDKROOT_FOR_TARGET
              fi
            '';
          };
        };
      }
    );
}
