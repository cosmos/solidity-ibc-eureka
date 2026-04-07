{
  lib,
  stdenv,
  fetchFromGitHub,
  rustPlatform,
  pkg-config,
  openssl,
  zlib,
  protobuf,
  perl,
  hidapi,
  udev,
  llvmPackages,
  rust-bin,
  solanaPkgs ? [
    "cargo-build-sbf"
    "cargo-test-sbf"
    "solana"
    "solana-bench-tps"
    "solana-faucet"
    "solana-gossip"
    "solana-keygen"
    "solana-log-analyzer"
    "solana-net-shaper"
    "solana-test-validator"
    "solana-genesis"
    "agave-ledger-tool"
    "agave-install"
    "agave-validator"
  ],
}: let
  inherit (lib) optionals;
  inherit (stdenv) hostPlatform isLinux;

  version = "2.3.13";

  rustForAgave = rust-bin.stable."1.86.0".default.override {
    extensions = ["rust-src"];
  };
in
  rustPlatform.buildRustPackage.override {
    rustc = rustForAgave;
    cargo = rustForAgave;
  } {
    pname = "agave";
    inherit version;

    src = fetchFromGitHub {
      owner = "anza-xyz";
      repo = "agave";
      rev = "v${version}";
      hash = "sha256-RSucqvbshaaby4fALhAQJtZztwsRdA+X7yRnoBxQvsg=";
      fetchSubmodules = true;
    };

    cargoHash = "sha256-yTS++bUu+4wmbXXZkU4eDq4sGNzls1euptJoY6OYZOM=";

    cargoBuildFlags = map (n: "--bin=${n}") solanaPkgs;

    nativeBuildInputs = [
      pkg-config
      protobuf
      perl
      llvmPackages.clang
    ];

    buildInputs =
      [
        openssl
        zlib
        llvmPackages.libclang.lib
      ]
      ++ optionals isLinux [
        hidapi
        udev
      ];

    LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

    BINDGEN_EXTRA_CLANG_ARGS = toString (
      [
        "-isystem ${llvmPackages.libclang.lib}/lib/clang/${lib.getVersion llvmPackages.clang}/include"
      ]
      ++ optionals isLinux [
        "-isystem ${stdenv.cc.libc.dev}/include"
      ]
      ++ optionals hostPlatform.isDarwin [
        "-isystem ${stdenv.cc.libc}/include"
      ]
    );

    postPatch = ''
      substituteInPlace scripts/cargo-install-all.sh \
        --replace-fail './fetch-perf-libs.sh' 'echo "Skipping fetch-perf-libs in Nix build"' \
        --replace-fail '"$cargo" $maybeRustVersion install' 'echo "Skipping cargo install"'
    '';

    doCheck = false;

    meta = with lib; {
      description = "Solana/Agave CLI tools";
      homepage = "https://github.com/anza-xyz/agave";
      license = licenses.asl20;
      platforms = platforms.unix;
    };
  }
