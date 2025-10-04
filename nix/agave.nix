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

      # Create env.sh at the root to fix strip.sh script path
      if [ -f "$out/sbf-sdk/env.sh" ]; then
        ln -s $out/sbf-sdk/env.sh $out/env.sh
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

  # Use Rust nightly version that's compatible with Agave version
  rustForAgave = rust-bin.nightly."2024-11-15".default.override {
    extensions = [ "rust-src" ];
  };

  agave = rustPlatform.buildRustPackage.override {
    rustc = rustForAgave;
    cargo = rustForAgave;
  } {
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

      # Make SBF target specs available to nightly rust for anchor idl build
      # anchor idl build runs cargo test which needs access to sbf-solana-solana target
      export RUST_TARGET_PATH="${platformTools}/rust/lib/rustlib"
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

      if cargo_toml=$(has_idl_build_feature); then
        echo "📝 Generating IDL with nightly toolchain..."

        setup_nightly

        echo "📤 Extracting IDL files..."
        mkdir -p target/idl

        local idl_success=0
        local idl_failed=0
        local idl_total=0

        # Extract IDL from each program using cargo test with idl-build feature
        for program_dir in programs/*/; do
          program_name=$(basename "$program_dir")

          has_idl_build=false
          if [ -f "$program_dir/Cargo.toml" ]; then
            if grep -q "idl-build" "$program_dir/Cargo.toml" 2>/dev/null || true; then
              has_idl_build=true
            fi
          fi

          if [ "$has_idl_build" = "true" ]; then
            ((idl_total++)) || true
            echo "   [✓] $program_name has idl-build feature ($idl_total)..."

            # Set required environment variables for IDL generation (from idl/src/build.rs:156-169)
            export ANCHOR_IDL_BUILD_PROGRAM_PATH="$program_dir"
            export ANCHOR_IDL_BUILD_RESOLUTION="TRUE"
            export ANCHOR_IDL_BUILD_NO_DOCS="FALSE"
            export ANCHOR_IDL_BUILD_SKIP_LINT="TRUE"
            export RUSTFLAGS="-A warnings"

            # Build this specific program with idl-build feature first
            echo "      Building with idl-build feature..."
            build_output=$(cargo build \
              --manifest-path "$program_dir/Cargo.toml" \
              --features idl-build \
              --lib 2>&1) || build_exit=$?
            build_exit=''${build_exit:-0}

            if [ "$build_exit" -ne 0 ]; then
              echo "   ⚠️  Build failed for $program_name, skipping IDL extraction"
              echo "   Build output (last 10 lines):" >&2
              echo "$build_output" | tail -10 >&2
              ((idl_failed++)) || true
              unset ANCHOR_IDL_BUILD_PROGRAM_PATH ANCHOR_IDL_BUILD_RESOLUTION
              unset ANCHOR_IDL_BUILD_NO_DOCS ANCHOR_IDL_BUILD_SKIP_LINT RUSTFLAGS
              continue
            fi

            echo "      Build succeeded, now extracting IDL..."
            temp_output="/tmp/idl_$program_name.txt"

            set +e  # Temporarily disable exit on error for test command
            cargo test \
              --manifest-path "$program_dir/Cargo.toml" \
              --features idl-build \
              --lib \
              __anchor_private_print_idl \
              -- \
              --show-output \
              --quiet \
              --test-threads=1 > "$temp_output" 2>&1
            test_exit=$?
            set -e  # Re-enable exit on error

            if [ "$test_exit" -eq 0 ]; then
              # Extract IDL JSON from program section (see idl/src/build.rs:202-280)
              idl_json=$(cat "$temp_output" | awk '
                BEGIN { in_program=0; program="" }
                /--- IDL begin program ---/ { in_program=1; next }
                /--- IDL end program ---/ { in_program=0; next }
                in_program { program = program $0 "\n" }
                END { printf "%s", program }
              ')

              if [ -n "$idl_json" ] && [ "$(echo "$idl_json" | tr -d '[:space:]')" != "" ]; then
                echo "$idl_json" > "target/idl/$program_name.json"
                echo "    ✓ Generated target/idl/$program_name.json"
                ((idl_success++)) || true
                rm -f "$temp_output"
              else
                echo "   ⚠️  Failed to extract IDL for $program_name (no program section found)"
                echo "   [DEBUG] First 30 lines of test output:"
                head -30 "$temp_output" >&2
                echo "   [DEBUG] Searching for IDL markers:"
                grep -n "IDL" "$temp_output" >&2 || echo "   No IDL markers found" >&2
                ((idl_failed++)) || true
                rm -f "$temp_output"
              fi
            else
              echo "   ❌ IDL test failed for $program_name (exit code: $test_exit)"
              echo "   [DEBUG] Last 30 lines of output:"
              tail -30 "$temp_output" >&2
              ((idl_failed++)) || true
              rm -f "$temp_output"
            fi

            # Clean up env vars
            unset ANCHOR_IDL_BUILD_PROGRAM_PATH ANCHOR_IDL_BUILD_RESOLUTION
            unset ANCHOR_IDL_BUILD_NO_DOCS ANCHOR_IDL_BUILD_SKIP_LINT RUSTFLAGS
          else
            echo "   [✗] $program_name does not have idl-build feature, skipping"
          fi
        done

        echo ""
        echo "📊 IDL Generation Summary: $idl_success succeeded, $idl_failed failed out of $idl_total total"

        # Report results and return appropriate exit code
        if [ "$idl_success" -gt 0 ] && [ "$idl_failed" -eq 0 ]; then
          echo "✅ Build complete: generated $idl_success IDL file(s)"
        elif [ "$idl_success" -gt 0 ] && [ "$idl_failed" -gt 0 ]; then
          echo "⚠️  Build complete: generated $idl_success IDL file(s), $idl_failed failed"
          return 1
        elif [ "$idl_failed" -gt 0 ]; then
          echo "❌ Build complete but all IDL generation failed ($idl_failed program(s))"
          return 1
        else
          echo "ℹ️  No programs with idl-build feature found"
        fi
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

    # Function to run unit tests with cargo test
    run_unit_test() {
      local extra_args=("$@")

      echo "🧪 Running unit tests..."

      if ! run_build; then
        return 1
      fi

      setup_nightly

      echo "🧪 Running cargo test with nightly toolchain..."
      cargo test "''${extra_args[@]}"
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

      unit-test)
        shift
        run_unit_test "$@"
        ;;

      *)
        cat <<EOF
anchor-nix: Anchor wrapper for Nix environments

Usage:
  anchor-nix build [options]      - Build program with Solana toolchain, generate IDL with nightly
  anchor-nix test [options]       - Build and run anchor client tests
  anchor-nix unit-test [options]  - Build program then run cargo test

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
