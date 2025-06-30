{ lib
, stdenv
, fetchFromGitHub
, fetchurl
, rustPlatform
, pkg-config
, openssl
, zlib
, protobuf
, rustfmt
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
    "agave-install"
    "solana-keygen"
    "agave-ledger-tool"
    "solana-log-analyzer"
    "solana-net-shaper"
    "agave-validator"
    "solana-test-validator"
    "solana-genesis"
  ]
}:

let
  # Create nightly toolchain from rust-bin
  rustNightly = rust-bin.nightly.latest.default.override {
    extensions = [ "rust-src" ];
  };

  platformToolsVersion = "v1.48";
  agave-version = "2.2.17";

  # Determine platform-tools archive based on system
  platformToolsArchive =
    if stdenv.isDarwin && stdenv.isx86_64 then "platform-tools-osx-x86_64.tar.bz2"
    else if stdenv.isDarwin && stdenv.isAarch64 then "platform-tools-osx-aarch64.tar.bz2"
    else if stdenv.isLinux && stdenv.isx86_64 then "platform-tools-linux-x86_64.tar.bz2"
    else if stdenv.isLinux && stdenv.isAarch64 then "platform-tools-linux-aarch64.tar.bz2"
    else throw "Unsupported platform for Solana platform-tools";

  platformTools = fetchurl {
    url = "https://github.com/anza-xyz/platform-tools/releases/download/${platformToolsVersion}/${platformToolsArchive}";
    sha256 =
      if stdenv.isDarwin && stdenv.isx86_64 then "sha256-0qik6gpvcq2rav1qy43n5vjipfa3m756p452y0fikir4cl5fvd5w="
      else if stdenv.isDarwin && stdenv.isAarch64 then "sha256-eZ5M/O444icVXIP7IpT5b5SoQ9QuAcA1n7cSjiIW0t0="
      else if stdenv.isLinux && stdenv.isx86_64 then "sha256-vHeOPs7B7WptUJ/mVvyt7ue+MqfqAsbwAHM+xlN/tgQ="
      else if stdenv.isLinux && stdenv.isAarch64 then "sha256-1wkh3vry4sc83ia8zfbv6yb6d7ygqsy88r1nj13y5fgp48i05imf="
      else throw "No hash for platform";
  };

  # Download SBF SDK from Agave releases
  sbfSdk = fetchurl {
    url = "https://github.com/anza-xyz/agave/releases/download/v${agave-version}/sbf-sdk.tar.bz2";
    sha256 = "18nh745djcnkbs0jz7bkaqrlwkbi5x28xdnr2lkgrpybwmdfg06s";
  };

  # Base agave package
  agave = rustPlatform.buildRustPackage rec {
    pname = "agave";
    version = agave-version;

    src = fetchFromGitHub {
      owner = "anza-xyz";
      repo = "agave";
      rev = "v${version}";
      hash = "sha256-Xbv00cfl40EctQhjIcysnkVze6aP5z2SKpzA2hWn54o=";
      fetchSubmodules = true;
    };

    cargoHash = "sha256-DEMbBkQPpeChmk9VtHq7asMrl5cgLYqNC/vGwrmdz3A=";

    cargoBuildFlags = builtins.map (n: "--bin=${n}") solanaPkgs;

    nativeBuildInputs = [
      pkg-config
      protobuf
      rustfmt
      perl
    ];

    buildInputs = [
      openssl
      zlib
    ] ++ lib.optionals stdenv.isLinux [
      hidapi
    ];

    postPatch = ''
      substituteInPlace scripts/cargo-install-all.sh \
        --replace './fetch-perf-libs.sh' 'echo "Skipping fetch-perf-libs in Nix build"'

      substituteInPlace scripts/cargo-install-all.sh \
        --replace '"$cargo" $maybeRustVersion install' 'echo "Skipping cargo install"'
    '';

    postInstall = ''
      # Extract platform-tools
      mkdir -p $out/bin
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
  };

  # Anchor-nix wrapper script that handles toolchain switching
  anchorNix = writeShellScriptBin "anchor-nix" ''
    #!/usr/bin/env bash

    # Store the original anchor path
    REAL_ANCHOR="${anchor}/bin/anchor"

    # Function to setup Solana toolchain
    setup_solana() {
      # Clean PATH of any rust toolchains
      export PATH=$(echo "$PATH" | tr ':' '\n' | grep -v "rust-bin" | grep -v ".cargo/bin" | grep -v "rustup" | tr '\n' ':')

      # Set up Agave environment
      export SBF_SDK_PATH="${agave}/sbf-sdk"
      export PATH="${agave}/bin/rust/bin:$PATH"
      export RUSTC="${agave}/bin/rust/bin/rustc"
      export CARGO="${agave}/bin/rust/bin/cargo"

      # Setup cache symlinks for cargo-build-sbf
      PLATFORM_TOOLS_VERSION="${platformToolsVersion}"
      CACHE_DIR="$HOME/.cache/solana/$PLATFORM_TOOLS_VERSION/platform-tools"
      mkdir -p "$CACHE_DIR"
      rm -rf "$CACHE_DIR/rust" "$CACHE_DIR/llvm" 2>/dev/null
      ln -sf "${agave}/bin/rust" "$CACHE_DIR/rust"
      ln -sf "${agave}/bin/llvm" "$CACHE_DIR/llvm"
      echo "$PLATFORM_TOOLS_VERSION" > "$CACHE_DIR/.version"
    }

    # Function to setup nightly toolchain
    setup_nightly() {
      # Clean PATH of any rust toolchains including agave
      export PATH=$(echo "$PATH" | tr ':' '\n' | grep -v "rust-bin" | grep -v ".cargo/bin" | grep -v "rustup" | grep -v "${agave}" | tr '\n' ':')

      # Unset Agave-specific environment variables
      unset RUSTC CARGO SBF_SDK_PATH

      # Add rust nightly to PATH
      export PATH="${rustNightly}/bin:$PATH"
    }

    case "$1" in
      build)
        echo "🔨 Building Solana program with optimized toolchain setup..."

        # First, build the program with Solana toolchain (no IDL)
        echo "📦 Building program with Solana/Agave toolchain..."
        setup_solana
        "$REAL_ANCHOR" build --no-idl -- --no-rustup-override --skip-tools-install "''${@:2}"

        BUILD_RESULT=$?

        if [[ $BUILD_RESULT -eq 0 ]]; then
          # Check if any program has idl-build feature
          echo "🔍 Checking for IDL build feature..."
          HAS_IDL_BUILD=false
          
          # Find all Cargo.toml files in programs directory
          for cargo_toml in programs/*/Cargo.toml; do
            if [[ -f "$cargo_toml" ]] && grep -q "idl-build" "$cargo_toml"; then
              HAS_IDL_BUILD=true
              echo "   Found idl-build feature in $cargo_toml"
              break
            fi
          done
          
          if [[ "$HAS_IDL_BUILD" == "true" ]]; then
            # If idl-build feature found, generate IDL with nightly toolchain
            echo "📝 Generating IDL with nightly toolchain..."
            setup_nightly
            cargo build --features=idl-build
            IDL_RESULT=$?

            if [[ $IDL_RESULT -eq 0 ]]; then
              echo "✅ Build complete: program built with Solana toolchain, IDL generated with nightly"
            else
              echo "⚠️  Program built successfully, but IDL generation failed"
              exit $IDL_RESULT
            fi
          else
            echo "ℹ️  Skipping IDL generation (no idl-build feature found in Cargo.toml)"
            echo "✅ Build complete: program built with Solana toolchain"
          fi
        else
          echo "❌ Program build failed"
          exit $BUILD_RESULT
        fi
        ;;

      test)
        echo "🧪 Testing Solana program with optimized toolchain setup..."

        # First, build with Solana toolchain
        echo "📦 Building program with Solana/Agave toolchain..."
        setup_solana
        "$REAL_ANCHOR" build --no-idl -- --no-rustup-override --skip-tools-install "''${@:2}"
        BUILD_RESULT=$?
        
        if [[ $BUILD_RESULT -eq 0 ]]; then
          # Check if any program has idl-build feature
          echo "🔍 Checking for IDL build feature..."
          HAS_IDL_BUILD=false
          
          # Find all Cargo.toml files in programs directory
          for cargo_toml in programs/*/Cargo.toml; do
            if [[ -f "$cargo_toml" ]] && grep -q "idl-build" "$cargo_toml"; then
              HAS_IDL_BUILD=true
              echo "   Found idl-build feature in $cargo_toml"
              break
            fi
          done
          
          # Switch to nightly for IDL (if needed) and tests
          setup_nightly
          
          if [[ "$HAS_IDL_BUILD" == "true" ]]; then
            echo "📝 Generating IDL with nightly toolchain..."
            "$REAL_ANCHOR" idl build "''${@:2}"
            IDL_RESULT=$?
            
            if [[ $IDL_RESULT -ne 0 ]]; then
              echo "⚠️  IDL generation failed"
              exit $IDL_RESULT
            fi
          else
            echo "ℹ️  Skipping IDL generation (no idl-build feature found)"
          fi
          
          # Run tests with nightly toolchain
          echo "🧪 Running tests with nightly toolchain..."
          "$REAL_ANCHOR" test --skip-build "''${@:2}"
          TEST_RESULT=$?
          
          if [[ $TEST_RESULT -eq 0 ]]; then
            echo "✅ All tests passed!"
          else
            echo "❌ Tests failed"
            exit $TEST_RESULT
          fi
        else
          echo "❌ Program build failed"
          exit $BUILD_RESULT
        fi
        ;;

      *)
        echo "anchor-nix: Optimized Anchor wrapper for Nix environments"
        echo ""
        echo "Usage:"
        echo "  anchor-nix build [options]  - Build program with Solana toolchain, generate IDL with nightly"
        echo "  anchor-nix test [options]   - Build and test program with optimized toolchain setup"
        echo ""
        echo "This wrapper automatically handles toolchain switching to provide:"
        echo "  - Fast, deterministic builds with Solana/Agave toolchain"
        echo "  - IDL generation with Rust nightly toolchain"
        echo ""
        echo "For other Anchor commands, use the regular 'anchor' command."
        exit 1
        ;;
    esac
  '';

in
# Combine everything into a single package
stdenv.mkDerivation {
  pname = "agave-with-toolchain";
  version = agave-version;

  dontUnpack = true;
  dontBuild = true;

  installPhase = ''
    mkdir -p $out/bin

    # Copy all binaries from agave
    cp -r ${agave}/bin/* $out/bin/

    # Copy other directories from agave
    cp -r ${agave}/sbf-sdk $out/

    # Add the wrapper scripts
    cp ${anchorNix}/bin/* $out/bin/
  '';

  meta = agave.meta // {
    description = "Solana programs & tooling";
  };
}
