{
  lib,
  stdenv,
  fetchFromGitHub,
  symlinkJoin,
  fetchurl,
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
  writeShellApplication,
  makeWrapper,
  anchor,
  jq,
  solanaPkgs ? [
    "cargo-build-sbf"
    "cargo-test-sbf"
    "solana"
    "solana-faucet"
    "solana-genesis"
    "solana-gossip"
    "solana-keygen"
    "solana-test-validator"
    "agave-install"
    "agave-validator"
  ],
}: let
  version = "3.1.6";
  # TODO: Update to v1.52+ when Anchor 1.0 is released with solana-sdk 3.x support.
  platformToolsVersion = "v1.48";
  system = stdenv.hostPlatform.system;

  # Rust 1.89.0 for IDL generation (Span::local_file was stabilized)
  rustIdl = rust-bin.stable."1.89.0".default.override {
    extensions = ["rust-src"];
  };

  # Rust 1.86.0 for building Agave
  rustForAgave = rust-bin.stable."1.86.0".default.override {
    extensions = ["rust-src"];
  };

  platformArchives = {
    x86_64-darwin = {
      name = "platform-tools-osx-x86_64.tar.bz2";
      sha256 = "sha256-vLTtCmUkxxkd8KKQa8qpQ7kb5S52EI/DVllgtu8zM2I=";
    };
    aarch64-darwin = {
      name = "platform-tools-osx-aarch64.tar.bz2";
      sha256 = "sha256-eZ5M/O444icVXIP7IpT5b5SoQ9QuAcA1n7cSjiIW0t0=";
    };
    x86_64-linux = {
      name = "platform-tools-linux-x86_64.tar.bz2";
      sha256 = "sha256-qdMVf5N9X2+vQyGjWoA14PgnEUpmOwFQ20kuiT7CdZc=";
    };
    aarch64-linux = {
      name = "platform-tools-linux-aarch64.tar.bz2";
      sha256 = "sha256-rsYCIiL3ueJHkDZkhLzGz59mljd7uY9UHIhp4vMecPI=";
    };
  };

  platformArchive = platformArchives.${system} or (throw "Unsupported platform: ${system}");

  # Prebuilt LLVM toolchain for compiling Solana programs to SBF bytecode
  platformTools = stdenv.mkDerivation {
    pname = "platform-tools";
    version = platformToolsVersion;
    src = fetchurl {
      url = "https://github.com/anza-xyz/platform-tools/releases/download/${platformToolsVersion}/${platformArchive.name}";
      inherit (platformArchive) sha256;
    };
    unpackPhase = ''
      mkdir -p $out
      tar -xjf $src -C $out
      # Remove dangling symlinks (ldb-argdumper)
      find $out -type l ! -exec test -e {} \; -delete 2>/dev/null || true
    '';
  };

  # Solana BPF SDK - runtime and build scripts for on-chain programs
  sbfSdk = stdenv.mkDerivation {
    pname = "sbf-sdk";
    inherit version;
    src = fetchurl {
      url = "https://github.com/anza-xyz/agave/releases/download/v${version}/sbf-sdk.tar.bz2";
      sha256 = "sha256-4iV6NhfisZuLlwwhIi4OIbxj8Nzx+EFcG5cmK36fFAc=";
    };
    unpackPhase = ''
      mkdir -p $out/dependencies
      tar -xjf $src -C $out
      ln -s ${platformTools} $out/dependencies/platform-tools
      if [ -d "${agave.src}/platform-tools-sdk/sbf/scripts" ]; then
        mkdir -p $out/scripts
        cp -r ${agave.src}/platform-tools-sdk/sbf/scripts/* $out/scripts/
        chmod +x $out/scripts/*.sh 2>/dev/null || true
      fi
      [ -f "$out/sbf-sdk/env.sh" ] && ln -s $out/sbf-sdk/env.sh $out/env.sh
    '';
  };

  agave =
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
        hash = "sha256-pIvShCRy1OQcFwSkXZ/lLF+2LoAd2wyAQfyyUtj9La0=";
        fetchSubmodules = true;
      };

      cargoHash = "sha256-eendPKd1oZmVqWAGWxm+AayLDm5w9J6/gSEPUXJZj88=";
      cargoBuildFlags = map (n: "--bin=${n}") solanaPkgs;

      nativeBuildInputs = [pkg-config protobuf perl llvmPackages.clang];

      buildInputs =
        [openssl zlib llvmPackages.libclang.lib]
        ++ lib.optionals stdenv.isLinux [hidapi udev];

      LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

      BINDGEN_EXTRA_CLANG_ARGS = toString (
        ["-isystem ${llvmPackages.libclang.lib}/lib/clang/${lib.getVersion llvmPackages.clang}/include"]
        ++ lib.optionals stdenv.isLinux ["-isystem ${stdenv.cc.libc.dev}/include"]
        ++ lib.optionals stdenv.isDarwin ["-isystem ${stdenv.cc.libc}/include"]
      );

      postPatch = ''
        substituteInPlace scripts/cargo-install-all.sh \
          --replace-fail './fetch-perf-libs.sh' 'echo "Skipping fetch-perf-libs in Nix build"' \
          --replace-fail '"$cargo" $maybeRustVersion install' 'echo "Skipping cargo install"'
      '';

      doCheck = false;

      meta = {
        description = "Solana cli and programs";
        homepage = "https://github.com/anza-xyz/agave";
        license = lib.licenses.asl20;
        platforms = lib.platforms.unix;
      };
    };

  # Wrapper script that uses platform-tools for building and rustIdl for IDL generation
  anchorNix = symlinkJoin {
    name = "anchor-nix";
    paths = [
      (writeShellApplication {
        name = "anchor-nix";
        runtimeInputs = [anchor agave jq];
        text = builtins.readFile ./anchor-nix.sh;
      })
    ];
    nativeBuildInputs = [makeWrapper];
    postBuild = ''
      wrapProgram $out/bin/anchor-nix \
        --set PLATFORM_TOOLS "${platformTools}" \
        --set RUST_IDL "${rustIdl}" \
        --set SBF_SDK_PATH "${sbfSdk}"
    '';
  };
in
  symlinkJoin {
    name = "agave-${version}";
    paths = [agave anchorNix anchor];
    passthru = {inherit agave rustIdl;};
    meta =
      agave.meta
      // {
        description = "Solana programs & tooling with Anchor wrapper";
        mainProgram = "anchor-nix";
      };
  }
