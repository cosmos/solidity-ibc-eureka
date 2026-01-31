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

  outputs = inputs:
    inputs.flake-utils.lib.eachSystem
    ["x86_64-linux" "aarch64-linux" "aarch64-darwin"]
    (
      system: let
        pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [
            (import inputs.rust-overlay)
            inputs.foundry.overlay
            inputs.solc.overlay
          ];
        };

        rust = import ./nix/rust.nix {inherit pkgs;};
        go = import ./nix/go.nix {inherit pkgs;};
        common = import ./nix/common.nix {inherit pkgs;};
        solidity = import ./nix/solidity.nix {inherit pkgs inputs system;};

        anchor = pkgs.callPackage ./nix/anchor.nix {};
        solana-agave = pkgs.callPackage ./nix/agave.nix {
          inherit (pkgs) rust-bin;
          inherit anchor;
        };
        anchor-go = pkgs.callPackage ./nix/anchor-go.nix {};
      in {
        devShells = {
          default = pkgs.mkShell {
            buildInputs = rust.packages ++ go.packages ++ common.packages ++ solidity.packages;
            inherit (rust) NIX_LD_LIBRARY_PATH;
            inherit (rust.env) RUST_SRC_PATH;
            shellHook =
              rust.shellHook
              + ''
                if [ -z "$(which cargo-prove)" ]; then
                  echo "SP1 toolchain is not installed. To install:"
                  echo "https://docs.succinct.xyz/docs/sp1/getting-started/install"
                fi
              '';
          };

          # Everything needed to work on the Solana part of this project
          solana = pkgs.mkShell {
            buildInputs =
              rust.packages
              ++ go.packages
              ++ common.packages
              ++ [solana-agave anchor-go];
            inherit (rust.env) RUST_SRC_PATH;
            shellHook =
              rust.shellHook
              + ''
                export PATH="${solana-agave}/bin:$PATH"
                echo "Solana shell: solana, anchor-nix (build|test|unit-test|keys|deploy)"
              '';
          };
        };
      }
    );
}
