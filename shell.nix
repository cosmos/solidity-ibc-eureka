{ pkgs ? import <nixpkgs> {}, lib ? pkgs.lib, stdenv ? pkgs.stdenv }:
let
  unstable = import
    (builtins.fetchTarball https://github.com/nixos/nixpkgs/tarball/ccc0c2126893dd20963580b6478d1a10a4512185)
    # reuse the current configuration
    { config = pkgs.config; };
in
  pkgs.mkShell {
    # nativeBuildInputs is usually what you want -- tools you need to run
    nativeBuildInputs = with pkgs.buildPackages; [ 
      openssl openssl.dev pkg-config
      solc-select unstable.bun go-ethereum
      unstable.golangci-lint unstable.go
      rustup just jq
    ];
    NIX_LD_LIBRARY_PATH = with pkgs.buildPackages; lib.makeLibraryPath [
      stdenv.cc.cc
    ];
    NIX_LD = lib.fileContents "${stdenv.cc}/nix-support/dynamic-linker";
    # Run a command after entering the shell
    shellHook = ''
      echo "Entering shell with solc 0.8.28"
      solc-select use 0.8.28
      echo "Entering shell with nightly rust"
      rustup override set nightly-2024-10-11
      # Check if tool is installed
      if [ -z "$(which cargo-prove)" ]; then
        echo "SP1 toolchain is not installed. Please follow the instructions at"
        echo "https://docs.succinct.xyz/getting-started/install.html"
      else
        echo "SP1 toolchain is already installed."
      fi
    '';
}
