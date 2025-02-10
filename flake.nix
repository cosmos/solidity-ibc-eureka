{
  description = "Development environment for the sp1 tendermint light client";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";

    solc = {
      url = "github:hellwolf/solc.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    foundry.url = "github:shazow/foundry.nix/main";
    rust-overlay.url = "github:oxalica/rust-overlay";
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
            rust-bin.stable.latest.default
            protobuf
            buf
            protoc-gen-go
            protoc-gen-go-grpc
            quicktype
          ];

          NIX_LD_LIBRARY_PATH = with pkgs.buildPackages; lib.makeLibraryPath [
            stdenv.cc.cc
          ];

          shellHook = ''
            if [ -z "$(which cargo-prove)" ]; then
              echo "SP1 toolchain is not installed. This is recommended to generate risc-v elfs. To install, please follow the instructions at"
              echo "https://docs.succinct.xyz/getting-started/install.html"
            fi
          '';
        };
      }
    );
}
