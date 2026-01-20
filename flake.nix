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

        rust = import ./pkgs/rust.nix {inherit pkgs;};
        go = import ./pkgs/go.nix {inherit pkgs;};

        anchor = pkgs.callPackage ./pkgs/anchor.nix {};
        solana-agave = pkgs.callPackage ./pkgs/agave.nix {
          inherit (pkgs) rust-bin;
          inherit anchor;
        };
        anchor-go = pkgs.callPackage ./pkgs/anchor-go.nix {};
      in {
        devShells = {
          default = pkgs.mkShell {
            buildInputs =
              rust.packages
              ++ go.packages
              ++ (with pkgs; [
                foundry-bin
                go-ethereum
                solc_0_8_28
                (inputs.solc.mkDefault pkgs solc_0_8_28)
                bun
                just
                jq
                parallel
                slither-analyzer
                quicktype
                inputs.natlint.packages.${system}.default
              ]);

            inherit (rust) NIX_LD_LIBRARY_PATH;
            inherit (rust.env) RUST_SRC_PATH;

            shellHook =
              rust.shellHook
              + go.shellHook
              + ''
                if [ -z "$(which cargo-prove)" ]; then
                  echo "SP1 toolchain is not installed. To install:"
                  echo "https://docs.succinct.xyz/docs/sp1/getting-started/install"
                fi
              '';
          };

          solana = pkgs.mkShell {
            buildInputs =
              rust.packages
              ++ go.packages
              ++ [
                pkgs.gawk
                pkgs.just
                solana-agave
                anchor-go
              ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
                pkgs.apple-sdk_15
              ];

            inherit (rust.env) RUST_SRC_PATH;

            shellHook =
              rust.shellHook
              + go.shellHook
              + ''
                export PATH="${solana-agave}/bin:$PATH"
                echo ""
                echo "Solana development shell activated"
                echo ""
                echo "Commands: solana, anchor-nix (build|test|unit-test|keys|deploy)"
                echo ""
              '';
          };
        };
      }
    );
}
