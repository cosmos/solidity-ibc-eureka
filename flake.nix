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
    sp1 = {
      url = "github:vaporif/sp1-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    anchor-overlay = {
      url = "github:vaporif/anchor-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
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
            inputs.sp1.overlays.default
            inputs.anchor-overlay.overlays.default
          ];
        };

        rust = import ./nix/rust.nix {inherit pkgs;};
        go = import ./nix/go.nix {inherit pkgs;};
        common = import ./nix/common.nix {inherit pkgs;};
        solidity = import ./nix/solidity.nix {inherit pkgs inputs system;};
        node-modules = import ./nix/node-modules.nix {inherit pkgs;};
        anchor-pkgs = pkgs.anchor."0.32.1";
        solana-agave = pkgs.callPackage ./nix/agave.nix {};
        anchor-go = pkgs.callPackage ./nix/anchor-go.nix {};
      in {
        devShells = {
          default = pkgs.mkShell {
            buildInputs =
              rust.packages
              ++ go.packages
              ++ common.packages
              ++ solidity.packages
              ++ [
                node-modules
              ]
              ++ (with pkgs.sp1."v5.2.4"; [
                cargo-prove
                sp1-rust-toolchain
              ]);
            inherit (rust) NIX_LD_LIBRARY_PATH;
            inherit (rust.env) RUST_SRC_PATH;
            shellHook =
              rust.shellHook
              + ''
                if [ -d "${node-modules}/node_modules" ]; then
                  if [ ! -e node_modules ] || [ -L node_modules ]; then
                    ln -sfn "${node-modules}/node_modules" node_modules
                  fi
                fi
              '';
          };

          # Everything needed to work on the Solana part of this project
          solana = pkgs.mkShell {
            buildInputs =
              rust.packages
              ++ go.packages
              ++ common.packages
              ++ [solana-agave anchor-pkgs.anchor-cli anchor-go node-modules];
            inherit (rust.env) RUST_SRC_PATH;
            shellHook =
              rust.shellHook
              + ''
                if [ -d "${node-modules}/node_modules" ]; then
                  if [ ! -e node_modules ] || [ -L node_modules ]; then
                    ln -sfn "${node-modules}/node_modules" node_modules
                  fi
                fi

                export PATH="${solana-agave}/bin:$PATH"
                echo "Solana shell: solana, anchor"
              '';
          };
        };
      }
    );
}
