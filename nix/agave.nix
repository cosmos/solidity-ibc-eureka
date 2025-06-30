{ lib
, stdenv
, fetchFromGitHub
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

  # Version management
  versions = {
    agave = "2.2.17";
    platformTools = "v1.48";
  };

  # Create nightly toolchain from rust-bin (used for IDL generation)
  rustNightly = rust-bin.nightly.latest.default.override {
    extensions = [ "rust-src" ];
  };

  # Platform-specific configuration
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

  platformTools = fetchurl {
    url = "https://github.com/anza-xyz/platform-tools/releases/download/${versions.platformTools}/${currentPlatform.archive}";
    inherit (currentPlatform) sha256;
  };

  # Download SBF SDK from Agave releases
  sbfSdk = fetchurl {
    url = "https://github.com/anza-xyz/agave/releases/download/v${versions.agave}/sbf-sdk.tar.bz2";
    sha256 = "18nh745djcnkbs0jz7bkaqrlwkbi5x28xdnr2lkgrpybwmdfg06s";
  };

  # Agave package bundled with Solana CLI & programs & toolchain & platform tools
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

    # Removed patches that don't exist - functionality is handled in postPatch
    postPatch = ''
      substituteInPlace scripts/cargo-install-all.sh \
        --replace-fail './fetch-perf-libs.sh' 'echo "Skipping fetch-perf-libs in Nix build"' \
        --replace-fail '"$cargo" $maybeRustVersion install' 'echo "Skipping cargo install"'
    '';

    postInstall = ''
      # Extract platform-tools
      tar -xjf ${platformTools} -C $out/bin/

      # Extract SBF SDK
      tar -xjf ${sbfSdk} -C $out/

      # The SBF SDK expects platform-tools to be in dependencies/platform-tools
      mkdir -p $out/sbf-sdk/dependencies
      ln -sf $out/bin $out/sbf-sdk/dependencies/platform-tools

      # Remove broken symlinks
      find $out/bin -type l ! -exec test -e {} \; -delete 2>/dev/null || true
    '';

    doCheck = false;

    meta = with lib; {
      description = "Solana runtime and toolchain";
      homepage = "https://github.com/anza-xyz/agave";
      license = licenses.asl20;
      maintainers = with maintainers; [ ];
      platforms = platforms.unix;
    };
  };

  # Anchor-nix wrapper script that handles toolchain switching
  anchorNix = writeShellScriptBin "anchor-nix" ''
    #!${stdenv.shell}
    set -euo pipefail

    # Store the original anchor path
    readonly REAL_ANCHOR="${anchor}/bin/anchor"
    readonly PLATFORM_TOOLS_VERSION="${versions.platformTools}"
    readonly AGAVE_PATH="${agave}"
    readonly RUST_NIGHTLY_PATH="${rustNightly}"

    # Function to clean PATH of rust toolchains
    clean_rust_from_path() {
      echo "$PATH" | tr ':' '\n' | \
        grep -v "rust-bin" | \
        grep -v ".cargo/bin" | \
        grep -v "rustup" | \
        tr '\n' ':'
    }

    # Function to setup Solana toolchain
    setup_solana() {
      # Clean PATH of any rust toolchains
      export PATH=$(clean_rust_from_path)

      # Set up Agave environment
      export SBF_SDK_PATH="$AGAVE_PATH/sbf-sdk"
      export PATH="$AGAVE_PATH/bin/rust/bin:$PATH"
      export RUSTC="$AGAVE_PATH/bin/rust/bin/rustc"
      export CARGO="$AGAVE_PATH/bin/rust/bin/cargo"

      # Setup cache symlinks for cargo-build-sbf
      local cache_dir="$HOME/.cache/solana/$PLATFORM_TOOLS_VERSION/platform-tools"
      mkdir -p "$cache_dir"
      
      # Use atomic operations for cache setup
      {
        rm -rf "$cache_dir/rust" "$cache_dir/llvm"
        ln -sf "$AGAVE_PATH/bin/rust" "$cache_dir/rust"
        ln -sf "$AGAVE_PATH/bin/llvm" "$cache_dir/llvm"
        echo "$PLATFORM_TOOLS_VERSION" > "$cache_dir/.version"
      } 2>/dev/null || true
    }

    # Function to setup nightly toolchain
    setup_nightly() {
      # Clean PATH including agave
      export PATH=$(clean_rust_from_path | sed "s|$AGAVE_PATH||g")

      # Unset Agave-specific environment variables
      unset RUSTC CARGO SBF_SDK_PATH || true

      # Add rust nightly to PATH
      export PATH="$RUST_NIGHTLY_PATH/bin:$PATH"
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

      echo "🧪 Testing Solana program with optimized toolchain setup..."

      # Build first
      if ! run_build "''${extra_args[@]}"; then
        return 1
      fi

      setup_nightly

      echo "🧪 Running tests with nightly toolchain..."
      if ! "$REAL_ANCHOR" test --skip-build "''${extra_args[@]}"; then
        echo "❌ Tests failed"
        return 1
      fi

      echo "✅ All tests passed!"
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
# Final derivation
stdenv.mkDerivation {
  pname = "agave-with-toolchain";
  version = versions.agave;

  dontUnpack = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall

    # Create output directory structure
    mkdir -p $out/bin

    # Copy everything from agave
    cp -a ${agave}/* $out/

    # Make bin directory writable
    chmod u+w $out/bin

    # Add the wrapper scripts
    cp -f ${anchorNix}/bin/* $out/bin/
    chmod 755 $out/bin/*

    runHook postInstall
  '';

  passthru = {
    inherit agave rustNightly;
    unwrapped = agave;
  };

  meta = agave.meta // {
    description = "Solana programs & tooling with Anchor wrapper";
    mainProgram = "anchor-nix";
  };
}
