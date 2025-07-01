{ lib
, stdenv
, fetchFromGitHub
, symlinkJoin
, fetchurl
, rustPlatform
, pkg-config
, openssl
, zlib
, protobuf
, perl
, hidapi
, rust-bin
, writeShellScriptBin
, anchor
, solanaPkgs ? [
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
  ]
}:

let
  inherit (lib) optionals;
  inherit (stdenv) hostPlatform isLinux;

  versions = {
    agave = "2.2.17";
    platformTools = "v1.48";
  };

  # Create nightly toolchain from rust-bin (used for IDL generation)
  rustNightly = rust-bin.nightly.latest.default.override {
    extensions = [ "rust-src" ];
  };

  platformConfig = {
    x86_64-darwin = {
      archive = "platform-tools-osx-x86_64.tar.bz2";
      sha256 = "sha256-0qik6gpvcq2rav1qy43n5vjipfa3m756p452y0fikir4cl5fvd5w=";
    };
    aarch64-darwin = {
      archive = "platform-tools-osx-aarch64.tar.bz2";
      sha256 = "sha256-eZ5M/O444icVXIP7IpT5b5SoQ9QuAcA1n7cSjiIW0t0=";
    };
    x86_64-linux = {
      archive = "platform-tools-linux-x86_64.tar.bz2";
      sha256 = "sha256-vHeOPs7B7WptUJ/mVvyt7ue+MqfqAsbwAHM+xlN/tgQ=";
    };
    aarch64-linux = {
      archive = "platform-tools-linux-aarch64.tar.bz2";
      sha256 = "sha256-1wkh3vry4sc83ia8zfbv6yb6d7ygqsy88r1nj13y5fgp48i05imf=";
    };
  };

  currentPlatform = platformConfig.${hostPlatform.system} or
    (throw "Unsupported platform: ${hostPlatform.system}");

  platformToolsArchive = fetchurl {
    url = "https://github.com/anza-xyz/platform-tools/releases/download/${versions.platformTools}/${currentPlatform.archive}";
    inherit (currentPlatform) sha256;
  };

  # Download SBF SDK archive from Agave releases
  sbfSdkArchive = fetchurl {
    url = "https://github.com/anza-xyz/agave/releases/download/v${versions.agave}/sbf-sdk.tar.bz2";
    sha256 = "18nh745djcnkbs0jz7bkaqrlwkbi5x28xdnr2lkgrpybwmdfg06s";
  };

  # SBF SDK derivation
  sbfSdk = stdenv.mkDerivation {
    pname = "sbf-sdk";
    version = versions.agave;

    src = sbfSdkArchive;

    unpackPhase = ''
      mkdir -p $out
      tar -xjf $src -C $out

      # Create symlink to platform tools
      mkdir -p $out/dependencies
      ln -s ${platformTools} $out/dependencies/platform-tools

      # Extract scripts from agave/platform-tools-sdk/sbf/scripts/
      if [ -d "${agave.src}/platform-tools-sdk/sbf/scripts" ]; then
        mkdir -p $out/scripts
        cp -r ${agave.src}/platform-tools-sdk/sbf/scripts/* $out/scripts/
        chmod +x $out/scripts/*.sh 2>/dev/null || true
      fi
    '';

    meta = with lib; {
      description = "Solana BPF SDK for building on-chain programs";
      homepage = "https://github.com/anza-xyz/agave";
      license = licenses.asl20;
      maintainers = with maintainers; [ vaporif ];
      platforms = platforms.unix;
    };
  };

  platformTools = stdenv.mkDerivation {
    pname = "platformTools";
    version = versions.platformTools;

    src = platformToolsArchive;

    unpackPhase = ''
      mkdir -p $out
      tar -xjf $src -C $out

      # ldb-argdumper in this package will point to a dangling link
      # we're only building not debugging so safe to just ignore
      find $out -type l ! -exec test -e {} \; -delete 2>/dev/null || true
    '';

    meta = with lib; {
      description = "Solana platform tools for building on-chain programs";
      homepage = "https://github.com/anza-xyz/platformTools";
      license = licenses.asl20;
      maintainers = with maintainers; [ vaporif ];
      platforms = platforms.unix;
    };
  };

  agave = rustPlatform.buildRustPackage {
    pname = "agave";
    version = versions.agave;

    src = fetchFromGitHub {
      owner = "anza-xyz";
      repo = "agave";
      rev = "v${versions.agave}";
      hash = "sha256-Xbv00cfl40EctQhjIcysnkVze6aP5z2SKpzA2hWn54o=";
      fetchSubmodules = true;
    };

    cargoHash = "sha256-DEMbBkQPpeChmk9VtHq7asMrl5cgLYqNC/vGwrmdz3A=";

    cargoBuildFlags = map (n: "--bin=${n}") solanaPkgs;

    nativeBuildInputs = [
      pkg-config
      protobuf
      perl
    ];

    buildInputs = [
      openssl
      zlib
    ] ++ optionals isLinux [ hidapi ];

    postPatch = ''
      substituteInPlace scripts/cargo-install-all.sh \
        --replace-fail './fetch-perf-libs.sh' 'echo "Skipping fetch-perf-libs in Nix build"' \
        --replace-fail '"$cargo" $maybeRustVersion install' 'echo "Skipping cargo install"'
    '';

    doCheck = false;

    meta = with lib; {
      description = "Solana cli and programs";
      homepage = "https://github.com/anza-xyz/agave";
      license = licenses.asl20;
      maintainers = with maintainers; [ vaporif ];
      platforms = platforms.unix;
    };
  };

  # Anchor-nix wrapper script
  #
  # Why we need this wrapper:
  # 1. Anchor CLI requires different Rust toolchains for different operations:
  #    - Building Solana programs: Requires the specific Solana/Agave Rust toolchain
  #      that is forked and maintained by Solana
  #    - Generating IDLs: Requires Rust nightly
  #    - Tests: Requires Rust nightly
  # 2. The wrapper intelligently switches between toolchains:
  #    - Strips existing Rust paths from PATH to avoid conflicts
  #    - Sets up the correct toolchain for each operation
  #    - No need to add rustup shims
  #    - Handles the complexity of toolchain management transparently
  #
  anchorNix = writeShellScriptBin "anchor-nix" ''
    #!${stdenv.shell}
    set -euo pipefail

    readonly REAL_ANCHOR="${anchor}/bin/anchor"
    export SBF_SDK_PATH="${sbfSdk}"

    clean_rust_from_path() {
      echo "$PATH" | tr ':' '\n' | \
        grep -v "rust-bin" | \
        grep -v ".cargo/bin" | \
        grep -v "rustup" | \
        tr '\n' ':'
    }

    setup_solana() {
      export PATH=$(clean_rust_from_path)

      export PATH="${platformTools}/rust/bin:$PATH"
      export RUSTC="${platformTools}/rust/bin/rustc"
      export CARGO="${platformTools}/rust/bin/cargo"
    }

    setup_nightly() {
      export PATH=$(clean_rust_from_path | sed "s|${platformTools}||g")

      unset RUSTC CARGO || true

      export PATH="${rustNightly}/bin:$PATH"
    }

    has_idl_build_feature() {
      find programs -name "Cargo.toml" -type f 2>/dev/null | \
        xargs grep -l "idl-build" 2>/dev/null | \
        head -n1
    }

    run_build() {
      local extra_args=("$@")

      echo "🔨 Building Solana program with solana toolchain setup..."
      echo "📦 Building program with Solana/Agave toolchain..."

       setup_solana

      if ! "$REAL_ANCHOR" build --no-idl -- --no-rustup-override --skip-tools-install "''${extra_args[@]}"; then
        echo "❌ Program build failed"
        return 1
      fi

      echo "🔍 Checking for IDL build feature..."
      if cargo_toml=$(has_idl_build_feature); then
        echo "   Found idl-build feature in $cargo_toml"
        echo "📝 Generating IDL with nightly toolchain..."

        setup_nightly

        if ! cargo build --features=idl-build; then
          echo "⚠️  Program built successfully, but IDL generation failed"
          return 1
        fi

        echo "✅ Build complete: program built with Solana toolchain, IDL generated with nightly"
      else
        echo "ℹ️  Skipping IDL generation (no idl-build feature found in Cargo.toml)"
        echo "✅ Build complete: program built with Solana toolchain"
      fi
    }

    # Function to run tests
    run_test() {
      local extra_args=("$@")

      echo "🧪 Testing Solana program..."

      if ! run_build "''${extra_args[@]}"; then
        return 1
      fi

      setup_nightly

      echo "🧪 Running tests with nightly toolchain..."
      "$REAL_ANCHOR" test --skip-build "''${extra_args[@]}"
    }

    # Main command dispatcher
    case "''${1:-}" in
      build)
        shift
        run_build "$@"
        ;;

      test)
        shift
        run_test "$@"
        ;;

      *)
        cat <<EOF
anchor-nix: Anchor wrapper for Nix environments

Usage:
  anchor-nix build [options]  - Build program with Solana toolchain, generate IDL with nightly
  anchor-nix test [options]   - Build and test program with optimized toolchain setup

This wrapper automatically handles toolchain switching to provide:
  - Fast, deterministic builds with Solana/Agave toolchain
  - IDL generation with Rust nightly toolchain

EOF
        exit 1
        ;;
    esac
  '';

in
symlinkJoin {
  name = "agave-with-toolchain-${versions.agave}";
  paths = [ agave anchorNix anchor ];

  passthru = {
    inherit agave rustNightly;
  };

  meta = agave.meta // {
    description = "Solana programs & tooling with Anchor wrapper";
    mainProgram = "anchor-nix";
  };
}
