#!/usr/bin/env bash
set -euo pipefail

# Required environment variables (set by Nix wrapper):
# shellcheck disable=SC2154
: "${PLATFORM_TOOLS:?PLATFORM_TOOLS not set}"
# shellcheck disable=SC2154
: "${RUST_IDL:?RUST_IDL not set}"
# shellcheck disable=SC2154
: "${SBF_SDK_PATH:?SBF_SDK_PATH not set}"
export SBF_SDK_PATH

clean_rust_from_path() {
  echo "${PATH}" | tr ':' '\n' | \
    grep -v "rust-bin" | \
    grep -v "fenix" | \
    grep -v ".cargo/bin" | \
    grep -v "rustup" | \
    tr '\n' ':'
}

setup_solana() {
  local new_path
  new_path=$(clean_rust_from_path)
  export PATH="${PLATFORM_TOOLS}/rust/bin:${new_path}"
  export RUSTC="${PLATFORM_TOOLS}/rust/bin/rustc"
  export CARGO="${PLATFORM_TOOLS}/rust/bin/cargo"
}

setup_idl() {
  local new_path
  new_path=$(clean_rust_from_path | sed "s|${PLATFORM_TOOLS}||g")
  unset RUSTC CARGO || true
  export PATH="${RUST_IDL}/bin:${new_path}"
  export RUST_TARGET_PATH="${PLATFORM_TOOLS}/rust/lib/rustlib"
}

has_idl_build_feature() {
  find programs -name "Cargo.toml" -type f -print0 2>/dev/null | \
    xargs -0 grep -l "idl-build" 2>/dev/null | \
    head -n1
}

run_build() {
  local extra_args=("$@")

  echo "Building program with Solana/Agave toolchain..."
  setup_solana

  local skip_idl=false
  local cargo_args=()
  local specific_package=""

  for arg in "${extra_args[@]}"; do
    if [[ "${arg}" == "--no-idl" ]]; then
      skip_idl=true
    elif [[ "${arg}" != "--" ]]; then
      cargo_args+=("${arg}")
    fi
  done

  for ((i=0; i<${#cargo_args[@]}; i++)); do
    if [[ "${cargo_args[${i}]}" == "-p" && $((i+1)) -lt ${#cargo_args[@]} ]]; then
      specific_package="${cargo_args[$((i+1))]}"
      echo "Building package: ${specific_package}"
      break
    fi
  done

  if [[ -n "${specific_package}" ]]; then
    local program_dir=""
    for dir in programs/*/; do
      if [[ "$(basename "${dir}")" == "${specific_package}" ]]; then
        program_dir="${dir}"
        break
      fi
    done

    if [[ -z "${program_dir}" ]]; then
      echo "Program directory not found for: ${specific_package}"
      return 1
    fi

    if ! cargo build-sbf --manifest-path "${program_dir}Cargo.toml" --no-rustup-override --skip-tools-install; then
      echo "Program build failed"
      return 1
    fi
  else
    if ! anchor build --no-idl -- --no-rustup-override --skip-tools-install; then
      echo "Program build failed"
      return 1
    fi
  fi

  if [[ "${skip_idl}" == true ]]; then
    echo "Skipping IDL generation (--no-idl flag)"
    return 0
  fi

  set +e
  has_idl_build_feature > /dev/null
  has_idl_result=$?
  set -e
  if [[ "${has_idl_result}" -eq 0 ]]; then
    echo "Generating IDL with stable toolchain..."
    setup_idl

    echo "Extracting IDL files..."
    mkdir -p target/idl

    local idl_success=0
    local idl_failed=0
    local idl_total=0

    for program_dir in programs/*/; do
      program_name=$(basename "${program_dir}")

      if [[ -n "${specific_package}" ]] && [[ "${program_name}" != "${specific_package}" ]]; then
        continue
      fi

      has_idl_build=false
      if [[ -f "${program_dir}/Cargo.toml" ]]; then
        if grep -q "idl-build" "${program_dir}/Cargo.toml" 2>/dev/null || true; then
          has_idl_build=true
        fi
      fi

      if [[ "${has_idl_build}" == "true" ]]; then
        ((idl_total++)) || true
        echo "  ${program_name} has idl-build feature (${idl_total})..."

        export ANCHOR_IDL_BUILD_PROGRAM_PATH="${program_dir}"
        export ANCHOR_IDL_BUILD_RESOLUTION="TRUE"
        export ANCHOR_IDL_BUILD_NO_DOCS="FALSE"
        export ANCHOR_IDL_BUILD_SKIP_LINT="TRUE"
        export RUSTFLAGS="-A warnings"

        echo "    Building with idl-build feature..."
        build_exit=0
        build_output=$(cargo build \
          --manifest-path "${program_dir}/Cargo.toml" \
          --features idl-build \
          --lib 2>&1) || build_exit=$?

        if [[ "${build_exit}" -ne 0 ]]; then
          echo "  Build failed for ${program_name}, skipping IDL extraction"
          echo "  Build output (last 10 lines):" >&2
          echo "${build_output}" | tail -10 >&2
          ((idl_failed++)) || true
          unset ANCHOR_IDL_BUILD_PROGRAM_PATH ANCHOR_IDL_BUILD_RESOLUTION
          unset ANCHOR_IDL_BUILD_NO_DOCS ANCHOR_IDL_BUILD_SKIP_LINT RUSTFLAGS
          continue
        fi

        echo "    Build succeeded, extracting IDL..."
        temp_output="/tmp/idl_${program_name}.txt"

        set +e
        cargo test \
          --manifest-path "${program_dir}/Cargo.toml" \
          --features idl-build \
          --lib \
          __anchor_private_print_idl \
          -- \
          --show-output \
          --quiet \
          --test-threads=1 > "${temp_output}" 2>&1
        test_exit=$?
        set -e

        if [[ "${test_exit}" -eq 0 ]]; then
          idl_json=$(awk '
            BEGIN { in_program=0; program="" }
            /--- IDL begin program ---/ { in_program=1; next }
            /--- IDL end program ---/ { in_program=0; next }
            in_program { program = program $0 "\n" }
            END { printf "%s", program }
          ' "${temp_output}")

          idl_stripped=$(echo "${idl_json}" | tr -d '[:space:]') || true
          if [[ -n "${idl_json}" ]] && [[ "${idl_stripped}" != "" ]]; then
            idl_filename=$(echo "${program_name}" | tr '-' '_')

            keypair_file="target/deploy/${idl_filename}-keypair.json"
            if [[ -f "${keypair_file}" ]]; then
              program_id=$(solana-keygen pubkey "${keypair_file}")
              idl_json=$(echo "${idl_json}" | jq --arg addr "${program_id}" '. + {address: $addr}')
              echo "    Added program ID: ${program_id}"
            fi

            echo "${idl_json}" > "target/idl/${idl_filename}.json"
            echo "    Generated target/idl/${idl_filename}.json"
            ((idl_success++)) || true
            rm -f "${temp_output}"
          else
            echo "  Failed to extract IDL for ${program_name} (no program section found)"
            ((idl_failed++)) || true
            rm -f "${temp_output}"
          fi
        else
          echo "  IDL test failed for ${program_name} (exit code: ${test_exit})"
          ((idl_failed++)) || true
          rm -f "${temp_output}"
        fi

        unset ANCHOR_IDL_BUILD_PROGRAM_PATH ANCHOR_IDL_BUILD_RESOLUTION
        unset ANCHOR_IDL_BUILD_NO_DOCS ANCHOR_IDL_BUILD_SKIP_LINT RUSTFLAGS
      fi
    done

    if [[ "${idl_success}" -gt 0 ]] && [[ "${idl_failed}" -eq 0 ]]; then
      echo "Build complete: generated ${idl_success} IDL file(s)"
    elif [[ "${idl_success}" -gt 0 ]] && [[ "${idl_failed}" -gt 0 ]]; then
      echo "Build complete: generated ${idl_success} IDL file(s), ${idl_failed} failed"
      return 1
    elif [[ "${idl_failed}" -gt 0 ]]; then
      echo "Build complete but all IDL generation failed (${idl_failed} program(s))"
      return 1
    else
      echo "No programs with idl-build feature found"
    fi
  else
    echo "Skipping IDL generation (no idl-build feature found)"
    echo "Build complete"
  fi
}

run_test() {
  local extra_args=("$@")

  echo "Testing Solana program..."

  set +e
  run_build "${extra_args[@]}"
  build_result=$?
  set -e
  if [[ "${build_result}" -ne 0 ]]; then
    return 1
  fi

  setup_idl

  echo "Running tests..."
  anchor test --skip-build "${extra_args[@]}"
}

run_unit_test() {
  local extra_args=("$@")

  echo "Running unit tests..."

  set +e
  run_build
  build_result=$?
  set -e
  if [[ "${build_result}" -ne 0 ]]; then
    return 1
  fi

  setup_idl

  echo "Running cargo test..."
  cargo test "${extra_args[@]}"
}

case "${1:-}" in
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
  keys)
    anchor "$@"
    ;;
  deploy)
    anchor "$@"
    ;;
  *)
    cat <<EOF
anchor-nix: Anchor wrapper for Nix environments

Usage:
  anchor-nix build [options]      - Build program with Solana toolchain, generate IDL
  anchor-nix test [options]       - Build and run anchor client tests
  anchor-nix unit-test [options]  - Build program then run cargo test
  anchor-nix keys [subcommand]    - Manage program keypairs (sync, list, etc.)
  anchor-nix deploy [options]     - Deploy programs to specified cluster

EOF
    exit 1
    ;;
esac
