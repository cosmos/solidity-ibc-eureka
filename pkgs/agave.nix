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
  platformToolsVersion = "v1.52";
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
      sha256 = "sha256-HdTysfe1MWwvGJjzfHXtSV7aoIMzM0kVP+lV5Wg3kdE=";
    };
    aarch64-darwin = {
      name = "platform-tools-osx-aarch64.tar.bz2";
      sha256 = "sha256-Fyffsx6DPOd30B5wy0s869JrN2vwnYBSfwJFfUz2/QA=";
    };
    x86_64-linux = {
      name = "platform-tools-linux-x86_64.tar.bz2";
      sha256 = "sha256-izhh6T2vCF7BK2XE+sN02b7EWHo94Whx2msIqwwdkH4=";
    };
    aarch64-linux = {
      name = "platform-tools-linux-aarch64.tar.bz2";
      sha256 = "sha256-sfhbLsR+9tUPZoPjUUv0apUmlQMVUXjN+0i9aUszH5g=";
    };
  };

  platformArchive = platformArchives.${system} or (throw "Unsupported platform: ${system}");

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
