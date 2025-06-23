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
      in
      {
        devShell = pkgs.mkShell {
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
            protobuf
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
      }
    );
}
