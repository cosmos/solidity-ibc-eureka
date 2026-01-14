set dotenv-load

# Detect which anchor command is available
anchor_cmd := `command -v anchor-nix >/dev/null 2>&1 && echo "anchor-nix" || echo "anchor"`

# Helper function to run solana-ibc CLI tool
solana_ibc := '''
  (cd tools/solana-ibc && go run . "$@")
'''

# Default task lists all available tasks
default:
  just --list

# Build the contracts using `forge build`
[group('build')]
build-contracts: clean-foundry
	forge build

# Build the relayer using `cargo build`
[group('build')]
build-relayer:
	cargo build --bin relayer --release --locked

# Build the operator using `cargo build`
[group('build')]
build-operator:
	cargo build --bin operator --release --locked

# Build the solana-ibc CLI tool using `go build`
[group('build')]
build-solana-ibc:
	cd tools/solana-ibc && go build -o ../../bin/solana-ibc .

# Build riscv elf files using `~/.sp1/bin/cargo-prove`
[group('build')]
build-sp1-programs:
  @echo "Building SP1 programs in 'programs/sp1-programs/target/elf-compilation/riscv32im-succinct-zkvm-elf/release/'"
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-update-client --locked
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-membership --locked
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-uc-and-membership --locked
  cd programs/sp1-programs && ~/.sp1/bin/cargo-prove prove build -p sp1-ics07-tendermint-misbehaviour --locked

# Sync Solana program keypairs and update declare_id! macros
# Usage: just sync-solana-keys [cluster]
# Example: just sync-solana-keys devnet
[group('solana')]
sync-solana-keys cluster="localnet": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  echo "Syncing Solana program keys for cluster: {{cluster}}"

  # Validate cluster directory exists
  if [ ! -d "solana-keypairs/{{cluster}}" ]; then
    echo "❌ Cluster directory not found: solana-keypairs/{{cluster}}"
    echo "   Available clusters: $(just list-clusters)"
    exit 1
  fi

  # Check for keypairs (skip for localnet as it's tracked in git)
  if [ "{{cluster}}" != "localnet" ] && [ ! -f "solana-keypairs/{{cluster}}/ics26_router-keypair.json" ]; then
    echo "❌ No keypairs found for cluster: {{cluster}}"
    echo "   Generate them first with: just generate-solana-keypairs {{cluster}}"
    exit 1
  fi

  # Add [programs.{{cluster}}] section to Anchor.toml if missing
  ANCHOR_TOML="programs/solana/Anchor.toml"
  if ! grep -q "^\[programs\.{{cluster}}\]" "$ANCHOR_TOML"; then
    echo "📝 Adding [programs.{{cluster}}] section to Anchor.toml..."

    # Build section content without trailing newline
    SECTION="[programs.{{cluster}}]"
    for keypair in solana-keypairs/{{cluster}}/*-keypair.json; do
      [ -f "$keypair" ] || continue
      PROGRAM_NAME=$(basename "$keypair" -keypair.json)
      PROGRAM_ID=$(solana-keygen pubkey "$keypair")
      SECTION+=$'\n'"$PROGRAM_NAME = \"$PROGRAM_ID\""
    done

    # Insert after [programs.localnet] section with proper spacing
    awk -v section="$SECTION" '
      /^\[programs\.localnet\]/ { in_localnet=1; print; next }
      in_localnet && /^[[:space:]]*$/ { next }
      in_localnet && /^\[/ {
        print ""
        print section
        print ""
        in_localnet=0
      }
      { print }
      END {
        if (in_localnet) {
          print ""
          print section
          print ""
        }
      }
    ' "$ANCHOR_TOML" > "$ANCHOR_TOML.tmp" && mv "$ANCHOR_TOML.tmp" "$ANCHOR_TOML"
  fi

  # Copy keypairs to target/deploy
  mkdir -p programs/solana/target/deploy
  cp -f solana-keypairs/{{cluster}}/*-keypair.json programs/solana/target/deploy/ 2>/dev/null || true

  # Sync declare_id! macros
  echo "🦀 Using {{anchor_cmd}}"
  (cd programs/solana && {{anchor_cmd}} keys sync --provider.cluster {{cluster}})

  echo "✅ Keys synced for cluster: {{cluster}}"

# Build Solana Anchor programs
# Usage: just build-solana [program]
# Example: just build-solana              (builds all programs)
# Example: just build-solana ics26-router (builds only ics26-router)
[group('solana')]
build-solana program="":
  #!/usr/bin/env bash
  set -euo pipefail

  if [ -z "{{program}}" ]; then
    echo "Building all programs..."
    echo "🦀 Using {{anchor_cmd}}"
    (cd programs/solana && {{anchor_cmd}} build)
    echo "✅ Build complete"
  else
    echo "Building program: {{program}}"
    PROGRAM_DIR="programs/solana/programs/{{program}}"

    if [ ! -d "$PROGRAM_DIR" ]; then
      echo "❌ Program directory not found: $PROGRAM_DIR"
      echo "   Available programs:"
      ls -1 programs/solana/programs/ | grep -v "^\." || true
      exit 1
    fi

    echo "🦀 Using {{anchor_cmd}}"

    # Build specific program and generate its IDL
    (cd programs/solana && {{anchor_cmd}} build -- -p "{{program}}")

    echo "✅ Build complete for {{program}}"
  fi

# Deploy Solana Anchor programs to a specific cluster (default: localnet)
# Usage: just deploy-solana [cluster] [max-len-multiplier]
# Example: just deploy-solana devnet
# Example: just deploy-solana localnet 3
# Use max-len-multiplier to reserve more space for future upgrades.
[group('solana')]
deploy-solana cluster="localnet" max_len_multiplier="2":
  #!/usr/bin/env bash
  set -euo pipefail

  echo "Deploying programs to {{cluster}}..."
  echo "🚀 Using {{anchor_cmd}}"
  echo "📏 Max length multiplier: {{max_len_multiplier}}x"

  # Get deployer wallet for localnet
  if [ "{{cluster}}" = "localnet" ]; then
    DEPLOYER_WALLET="$(pwd)/solana-keypairs/{{cluster}}/deployer_wallet.json"
    WALLET_ARG="--provider.wallet $DEPLOYER_WALLET"
  else
    WALLET_ARG=""
  fi

  # Deploy each program individually with its own max-len
  cd programs/solana
  for program_so in target/deploy/*.so; do
    if [ -f "$program_so" ]; then
      PROGRAM_NAME=$(basename "$program_so" .so)
      PROGRAM_SIZE=$(stat -f%z "$program_so" 2>/dev/null || stat -c%s "$program_so" 2>/dev/null)
      MAX_LEN=$((PROGRAM_SIZE * {{max_len_multiplier}}))

      echo "📦 Deploying $PROGRAM_NAME (size: $PROGRAM_SIZE bytes, max-len: $MAX_LEN bytes)"
      {{anchor_cmd}} deploy --provider.cluster {{cluster}} $WALLET_ARG -p "$PROGRAM_NAME" -- --max-len $MAX_LEN
    fi
  done
  cd ../..

  echo "✅ Deployment complete for cluster: {{cluster}}"

# Full deployment: deploy programs, initialize access manager, set upgrade authorities, and grant upgrader role
# Usage: just deploy-solana-full [cluster] [max-len-multiplier]
# Example: just deploy-solana-full localnet
# Example: just deploy-solana-full devnet 3
[group('solana')]
deploy-solana-full cluster="localnet" max_len_multiplier="2": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  echo "🚀 Starting full Solana deployment for cluster: {{cluster}}"
  echo ""

  # Step 1: Deploy all programs
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "📦 Step 1/4: Deploying Solana programs"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  just deploy-solana {{cluster}} {{max_len_multiplier}}
  echo ""

  # Step 2: Initialize AccessManager
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "🔑 Step 2/4: Initializing AccessManager"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  just initialize-access-manager "" {{cluster}}
  echo ""

  # Step 3: Set upgrade authority to AccessManager for all programs (except access_manager itself)
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "🔐 Step 3/4: Setting upgrade authorities to AccessManager"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  PROGRAMS=(
    "ics26_router"
    "ics27_gmp"
    "ics07_tendermint"
    "dummy_ibc_app"
    "mock_ibc_app"
    "gmp_counter_app"
    "mock_light_client"
  )

  for program in "${PROGRAMS[@]}"; do
    echo "Setting upgrade authority for $program..."
    just set-upgrade-authority "$program" {{cluster}} || {
      echo "⚠️  Warning: Failed to set upgrade authority for $program"
    }
  done
  echo ""

  # Step 4: Grant UPGRADER_ROLE (8) to deployer
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "👤 Step 4/4: Granting UPGRADER_ROLE (8) to deployer"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  just grant-solana-role 8 "" {{cluster}}
  echo ""

  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "✅ Full deployment complete for cluster: {{cluster}}"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""
  echo "Summary:"
  echo "  ✓ All programs deployed"
  echo "  ✓ AccessManager initialized"
  echo "  ✓ Upgrade authorities set to AccessManager PDAs"
  echo "  ✓ UPGRADER_ROLE granted to deployer"
  echo ""
  echo "Next steps:"
  echo "  • Use 'just upgrade-solana-program <program> {{cluster}}' to upgrade programs"
  echo "  • Use 'just grant-solana-role <role-id> <account> {{cluster}}' to grant additional roles"

# Initialize AccessManager with an admin
# Usage: just initialize-access-manager [admin-pubkey] [cluster]
# Example: just initialize-access-manager
# Example: just initialize-access-manager 8ntLtUdGwBaXfFPCrNis9MWsKMdEUYyonwuw7NQwhs5z
[group('solana')]
initialize-access-manager admin_pubkey="" cluster="localnet": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  # Get cluster URL from Anchor.toml
  CLUSTER_URL=$(just get-cluster-url {{cluster}})

  # Get payer keypair (deployer for localnet, or specified payer)
  PAYER_KEYPAIR="solana-keypairs/{{cluster}}/deployer_wallet.json"

  # Default admin to deployer if not specified
  if [ -z "{{admin_pubkey}}" ]; then
    ADMIN_PUBKEY=$(solana-keygen pubkey "$PAYER_KEYPAIR")
  else
    ADMIN_PUBKEY="{{admin_pubkey}}"
  fi

  if [ ! -f "$PAYER_KEYPAIR" ]; then
    echo "❌ Payer keypair not found: $PAYER_KEYPAIR"
    exit 1
  fi

  # Get access manager program ID
  ACCESS_MANAGER_ID=$(solana-keygen pubkey "solana-keypairs/{{cluster}}/access_manager-keypair.json")

  echo "Initializing AccessManager"
  echo "Cluster: {{cluster}} ($CLUSTER_URL)"
  echo "Admin: $ADMIN_PUBKEY"
  echo "Access Manager: $ACCESS_MANAGER_ID"
  echo ""

  # Convert keypair to absolute path
  ABS_PAYER_KEYPAIR=$(cd "$(dirname "$PAYER_KEYPAIR")" && pwd)/$(basename "$PAYER_KEYPAIR")

  solana_ibc() {
    {{solana_ibc}}
  }
  solana_ibc access-manager initialize "$CLUSTER_URL" "$ABS_PAYER_KEYPAIR" "$ADMIN_PUBKEY" "$ACCESS_MANAGER_ID"

# Grant a role to an account
# Usage: just grant-solana-role <role-id> [account-pubkey] [cluster]
# Example: just grant-solana-role 8
# Example: just grant-solana-role 8 Abc123...
[group('solana')]
grant-solana-role role_id account="" cluster="localnet": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  # Get cluster URL from Anchor.toml
  CLUSTER_URL=$(just get-cluster-url {{cluster}})

  # Get admin keypair (deployer for localnet, or specified admin)
  ADMIN_KEYPAIR="solana-keypairs/{{cluster}}/deployer_wallet.json"

  if [ ! -f "$ADMIN_KEYPAIR" ]; then
    echo "❌ Admin keypair not found: $ADMIN_KEYPAIR"
    exit 1
  fi

  # Default account to deployer if not specified
  if [ -z "{{account}}" ]; then
    ACCOUNT_PUBKEY=$(solana-keygen pubkey "$ADMIN_KEYPAIR")
  else
    ACCOUNT_PUBKEY="{{account}}"
  fi

  # Get access manager program ID
  ACCESS_MANAGER_ID=$(solana-keygen pubkey "solana-keypairs/{{cluster}}/access_manager-keypair.json")

  echo "Granting role {{role_id}} to $ACCOUNT_PUBKEY"
  echo "Cluster: {{cluster}} ($CLUSTER_URL)"
  echo "Access Manager: $ACCESS_MANAGER_ID"
  echo ""

  # Convert keypair to absolute path
  ABS_ADMIN_KEYPAIR=$(cd "$(dirname "$ADMIN_KEYPAIR")" && pwd)/$(basename "$ADMIN_KEYPAIR")

  solana_ibc() {
    {{solana_ibc}}
  }
  solana_ibc access-manager grant "$CLUSTER_URL" "$ABS_ADMIN_KEYPAIR" "{{role_id}}" "$ACCOUNT_PUBKEY" "$ACCESS_MANAGER_ID"

# Set program upgrade authority to AccessManager PDA
# Usage: just set-upgrade-authority <program-name> [cluster] [current-authority-keypair]
# Example: just set-upgrade-authority ics26_router
# Example: just set-upgrade-authority ics26_router localnet ~/.config/solana/id.json
[group('solana')]
set-upgrade-authority program_name cluster="localnet" current_authority="": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  # Get cluster URL from Anchor.toml
  CLUSTER_URL=$(just get-cluster-url {{cluster}})

  # Get program ID from keypair
  PROGRAM_KEYPAIR="solana-keypairs/{{cluster}}/{{program_name}}-keypair.json"
  if [ ! -f "$PROGRAM_KEYPAIR" ]; then
    echo "❌ Program keypair not found: $PROGRAM_KEYPAIR"
    exit 1
  fi
  PROGRAM_ID=$(solana-keygen pubkey "$PROGRAM_KEYPAIR")

  # Determine current authority keypair (default to deployer)
  if [ -n "{{current_authority}}" ]; then
    CURRENT_AUTHORITY="{{current_authority}}"
  else
    CURRENT_AUTHORITY="solana-keypairs/{{cluster}}/deployer_wallet.json"
    if [ ! -f "$CURRENT_AUTHORITY" ]; then
      echo "❌ Default authority keypair not found: $CURRENT_AUTHORITY"
      echo "   Please specify current authority explicitly"
      exit 1
    fi
  fi

  ACCESS_MANAGER_ID=$(solana-keygen pubkey "solana-keypairs/{{cluster}}/access_manager-keypair.json")

  # Derive upgrade authority PDA
  solana_ibc() {
    {{solana_ibc}}
  }
  UPGRADE_AUTH_PDA=$(solana_ibc upgrade derive-pda "$ACCESS_MANAGER_ID" "$PROGRAM_ID")

  echo "Setting upgrade authority for program {{program_name}}"
  echo "Program ID: $PROGRAM_ID"
  echo "Cluster: {{cluster}} ($CLUSTER_URL)"
  echo "Current authority: $CURRENT_AUTHORITY"
  echo "New authority (PDA): $UPGRADE_AUTH_PDA"
  echo ""

  solana program set-upgrade-authority "$PROGRAM_ID" \
    --upgrade-authority "$CURRENT_AUTHORITY" \
    --new-upgrade-authority "$UPGRADE_AUTH_PDA" \
    --skip-new-upgrade-authority-signer-check \
    --url "$CLUSTER_URL"

  echo ""
  echo "✅ Upgrade authority set to AccessManager PDA"
  echo "   Program: {{program_name}}"
  echo "   Program ID: $PROGRAM_ID"
  echo "   Upgrade Authority: $UPGRADE_AUTH_PDA"

  echo "✅ Upgrade authority set successfully!"

# Revoke a role from an account
# Usage: just revoke-solana-role <role-id> [account-pubkey] [cluster]
# Example: just revoke-solana-role 8
# Example: just revoke-solana-role 8 Abc123...
[group('solana')]
revoke-solana-role role_id account="" cluster="localnet": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  # Get cluster URL from Anchor.toml
  CLUSTER_URL=$(just get-cluster-url {{cluster}})

  # Get admin keypair
  ADMIN_KEYPAIR="solana-keypairs/{{cluster}}/deployer_wallet.json"

  if [ ! -f "$ADMIN_KEYPAIR" ]; then
    echo "❌ Admin keypair not found: $ADMIN_KEYPAIR"
    exit 1
  fi

  # Default account to deployer if not specified
  if [ -z "{{account}}" ]; then
    ACCOUNT_PUBKEY=$(solana-keygen pubkey "$ADMIN_KEYPAIR")
  else
    ACCOUNT_PUBKEY="{{account}}"
  fi

  # Get access manager program ID
  ACCESS_MANAGER_ID=$(solana-keygen pubkey "solana-keypairs/{{cluster}}/access_manager-keypair.json")

  echo "Revoking role {{role_id}} from $ACCOUNT_PUBKEY"
  echo "Cluster: {{cluster}} ($CLUSTER_URL)"
  echo "Access Manager: $ACCESS_MANAGER_ID"
  echo ""

  # Convert keypair to absolute path
  ABS_ADMIN_KEYPAIR=$(cd "$(dirname "$ADMIN_KEYPAIR")" && pwd)/$(basename "$ADMIN_KEYPAIR")

  solana_ibc() {
    {{solana_ibc}}
  }
  solana_ibc access-manager revoke "$CLUSTER_URL" "$ABS_ADMIN_KEYPAIR" "{{role_id}}" "$ACCOUNT_PUBKEY" "$ACCESS_MANAGER_ID"

# Prepare program upgrade buffer (steps 1-3 of upgrade process)
# Usage: just prepare-solana-upgrade <program-name> <upgrade-authority-pda> [cluster]
# Example: just prepare-solana-upgrade ics26_router GzT...xyz
# Note: Requires deployer wallet funded with SOL
[group('solana')]
prepare-solana-upgrade program upgrade_authority_pda cluster="localnet": build-solana
  #!/usr/bin/env bash
  set -euo pipefail

  PROGRAM_SO="programs/solana/target/deploy/{{program}}.so"
  DEPLOYER_KEYPAIR="solana-keypairs/{{cluster}}/deployer_wallet.json"
  CLUSTER_URL="{{cluster}}"

  # Validate inputs
  if [ ! -f "$PROGRAM_SO" ]; then
    echo "❌ Program binary not found: $PROGRAM_SO"
    echo "   Run 'just build-solana' first"
    exit 1
  fi

  if [ ! -f "$DEPLOYER_KEYPAIR" ]; then
    echo "❌ Deployer keypair not found: $DEPLOYER_KEYPAIR"
    exit 1
  fi

  echo "📦 Preparing upgrade for program: {{program}}"
  echo "   Cluster: {{cluster}}"
  echo "   Upgrade Authority PDA: {{upgrade_authority_pda}}"
  echo ""

  # Step 2: Write program to buffer
  echo "Step 1/2: Writing program to buffer..."
  BUFFER_OUTPUT=$(solana program write-buffer "$PROGRAM_SO" \
    --url "$CLUSTER_URL" \
    --keypair "$DEPLOYER_KEYPAIR" \
    --use-rpc 2>&1)

  # Extract buffer address from output
  BUFFER_ADDRESS=$(echo "$BUFFER_OUTPUT" | grep -oP 'Buffer: \K[A-Za-z0-9]+' || true)

  if [ -z "$BUFFER_ADDRESS" ]; then
    echo "❌ Failed to create buffer"
    echo "$BUFFER_OUTPUT"
    exit 1
  fi

  echo "✅ Buffer created: $BUFFER_ADDRESS"
  echo ""

  # Step 3: Set buffer authority
  echo "Step 2/2: Setting buffer authority to upgrade authority PDA..."
  solana program set-buffer-authority "$BUFFER_ADDRESS" \
    --new-buffer-authority "{{upgrade_authority_pda}}" \
    --buffer-authority "$DEPLOYER_KEYPAIR" \
    --keypair "$DEPLOYER_KEYPAIR" \
    --url "$CLUSTER_URL"

  echo ""
  echo "✅ Upgrade buffer prepared successfully!"
  echo ""
  echo "📋 Next steps:"
  echo "   Buffer Address: $BUFFER_ADDRESS"
  echo "   Upgrade Authority: {{upgrade_authority_pda}}"
  echo ""
  echo "   To complete the upgrade, call AccessManager.upgrade_program with:"
  echo "   - buffer: $BUFFER_ADDRESS"
  echo "   - target_program: <PROGRAM_ID>"
  echo "   - authority: <ACCOUNT_WITH_UPGRADER_ROLE>"
  echo ""
  echo "   See e2e/interchaintestv8/solana_upgrade_test.go for implementation example"

# Execute complete program upgrade (prepare buffer + execute upgrade instruction)
# Usage: just upgrade-solana-program <program-name> [cluster] [upgrader-keypair]
# Example: just upgrade-solana-program ics26_router
# Example: just upgrade-solana-program ics26_router devnet solana-keypairs/devnet/upgrader.json
# Note: Requires UPGRADER_ROLE granted to the upgrader keypair (defaults to deployer)
[group('solana')]
upgrade-solana-program program cluster="localnet" upgrader_keypair="": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  PROGRAM_SO="programs/solana/target/deploy/{{program}}.so"
  DEPLOYER_KEYPAIR="solana-keypairs/{{cluster}}/deployer_wallet.json"

  # Default upgrader to deployer if not specified
  if [ -n "{{upgrader_keypair}}" ]; then
    UPGRADER_KEYPAIR="{{upgrader_keypair}}"
  else
    UPGRADER_KEYPAIR="$DEPLOYER_KEYPAIR"
  fi

  # Get cluster URL from Anchor.toml
  CLUSTER_URL=$(just get-cluster-url {{cluster}})

  # Get program ID from keypair
  PROGRAM_KEYPAIR="solana-keypairs/{{cluster}}/{{program}}-keypair.json"
  if [ ! -f "$PROGRAM_KEYPAIR" ]; then
    echo "❌ Program keypair not found: $PROGRAM_KEYPAIR"
    exit 1
  fi
  PROGRAM_ID=$(solana-keygen pubkey "$PROGRAM_KEYPAIR")

  # Get access-manager program ID
  ACCESS_MANAGER_ID=$(solana-keygen pubkey "solana-keypairs/{{cluster}}/access_manager-keypair.json")

  # Derive upgrade authority PDA
  solana_ibc() {
    {{solana_ibc}}
  }
  UPGRADE_AUTH_PDA=$(solana_ibc upgrade derive-pda "$ACCESS_MANAGER_ID" "$PROGRAM_ID" 2>/dev/null || echo "")

  if [ -z "$UPGRADE_AUTH_PDA" ]; then
    echo "❌ Failed to derive upgrade authority PDA"
    exit 1
  fi

  echo "🔧 Starting program upgrade for: {{program}}"
  echo "   Program ID: $PROGRAM_ID"
  echo "   Cluster: {{cluster}} ($CLUSTER_URL)"
  echo "   Access Manager: $ACCESS_MANAGER_ID"
  echo "   Upgrade Authority PDA: $UPGRADE_AUTH_PDA"
  echo ""

  # Step 1-3: Prepare buffer
  echo "Step 1-3: Preparing upgrade buffer..."

  if ! BUFFER_OUTPUT=$(solana program write-buffer "$PROGRAM_SO" \
    --url "$CLUSTER_URL" \
    --keypair "$DEPLOYER_KEYPAIR" \
    --use-rpc 2>&1); then
    echo "❌ Failed to create buffer"
    echo "$BUFFER_OUTPUT"
    exit 1
  fi

  BUFFER_ADDRESS=$(echo "$BUFFER_OUTPUT" | grep -oP 'Buffer: \K[A-Za-z0-9]+' || true)

  if [ -z "$BUFFER_ADDRESS" ]; then
    echo "❌ Failed to extract buffer address from output"
    echo "$BUFFER_OUTPUT"
    exit 1
  fi

  echo "✅ Buffer created: $BUFFER_ADDRESS"

  # Set buffer authority to match upgrade authority PDA
  echo "Setting buffer authority to: $UPGRADE_AUTH_PDA"
  solana program set-buffer-authority "$BUFFER_ADDRESS" \
    --new-buffer-authority "$UPGRADE_AUTH_PDA" \
    --buffer-authority "$DEPLOYER_KEYPAIR" \
    --keypair "$DEPLOYER_KEYPAIR" \
    --url "$CLUSTER_URL"

  echo "✅ Buffer authority set to upgrade authority PDA"

  # Derive program data address
  PROGRAM_DATA_ADDR=$(solana program show "$PROGRAM_ID" --url "$CLUSTER_URL" | grep "ProgramData Address" | awk '{print $3}')

  if [ -z "$PROGRAM_DATA_ADDR" ]; then
    echo "❌ Failed to get program data address"
    exit 1
  fi

  echo "Program Data Address: $PROGRAM_DATA_ADDR"
  echo ""
  echo "Step 4: Executing upgrade instruction..."
  echo "(This requires UPGRADER_ROLE on the upgrader keypair)"
  echo ""

  # Convert upgrader keypair to absolute path
  ABS_UPGRADER_KEYPAIR=$(cd "$(dirname "$UPGRADER_KEYPAIR")" && pwd)/$(basename "$UPGRADER_KEYPAIR")

  # Call the upgrade tool
  solana_ibc upgrade program \
    "$CLUSTER_URL" \
    "$ABS_UPGRADER_KEYPAIR" \
    "$PROGRAM_ID" \
    "$BUFFER_ADDRESS" \
    "$ACCESS_MANAGER_ID" \
    "$PROGRAM_DATA_ADDR"

  echo ""
  echo "✅ Program upgrade complete!"

# Generate Solana keypairs for a specific cluster
# Usage: just generate-solana-keypairs <cluster>
# Example: just generate-solana-keypairs devnet
[group('solana')]
generate-solana-keypairs cluster="localnet": (_validate-cluster cluster)
  #!/usr/bin/env bash
  set -euo pipefail

  # Cannot regenerate localnet keypairs (tracked in git for E2E tests)
  if [ "{{cluster}}" = "localnet" ]; then
    echo "❌ Cannot generate keypairs for localnet - they are tracked in git"
    echo "   Localnet keypairs are used for E2E tests and should not be regenerated"
    exit 1
  fi

  echo "Generating keypairs for cluster: {{cluster}}"
  mkdir -p solana-keypairs/{{cluster}}
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/ics26_router-keypair.json
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/ics07_tendermint-keypair.json
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/ics27_gmp-keypair.json
  solana-keygen new --no-bip39-passphrase --force --outfile solana-keypairs/{{cluster}}/access_manager-keypair.json
  echo ""
  echo "✅ Keypairs generated in solana-keypairs/{{cluster}}/"
  echo "⚠️  IMPORTANT: Backup these keypairs securely! They are NOT tracked in git."
  echo ""
  echo "📋 Program IDs for {{cluster}}:"
  for keypair in solana-keypairs/{{cluster}}/*-keypair.json; do
    printf "   %-35s %s\n" "$(basename $keypair):" "$(solana-keygen pubkey $keypair)"
  done
  echo ""
  echo "Next step: just build-solana"

# Get cluster URL from Anchor.toml
[group('solana')]
get-cluster-url cluster="localnet": (_validate-cluster cluster)
  #!/usr/bin/env bash
  awk -F' = ' -v cluster="{{cluster}}" '
    /^\[clusters\]/ { in_clusters=1; next }
    in_clusters && /^\[/ { exit }
    in_clusters && $1 == cluster { gsub(/"/, "", $2); print $2; exit }
  ' programs/solana/Anchor.toml

# List available clusters from Anchor.toml
[group('solana')]
list-clusters:
  #!/usr/bin/env bash
  awk '
    /^\[clusters\]/ { in_clusters=1; next }
    in_clusters && /^\[/ { exit }
    in_clusters && /^[a-z]/ {
      split($0, parts, " = ")
      print parts[1]
    }
  ' programs/solana/Anchor.toml | tr '\n' ',' | sed 's/,$//'

# Validate cluster exists in Anchor.toml (internal helper recipe)
[private]
_validate-cluster cluster:
  #!/usr/bin/env bash
  if ! awk -F' = ' -v cluster="{{cluster}}" '
    /^\[clusters\]/ { in_clusters=1; next }
    in_clusters && /^\[/ { exit }
    in_clusters && $1 == cluster { found=1; exit }
    END { exit !found }
  ' programs/solana/Anchor.toml; then
    AVAILABLE=$(just list-clusters)
    echo "❌ Unknown cluster: {{cluster}}" >&2
    echo "   Available clusters: $AVAILABLE" >&2
    exit 1
  fi

# Build and optimize the eth wasm light client using a local docker image. Requires `docker` and `gzip`
[group('build')]
build-cw-ics08-wasm-eth:
  -@docker image rm cosmwasm-builder:latest
  cd programs/cw-ics08-wasm-eth && docker buildx build --platform linux/amd64 -t cosmwasm-builder:latest .
  docker run --rm --platform=linux/amd64  -t \
    -v "$PWD":/code \
    cosmwasm-builder:latest
  cp artifacts/cw_ics08_wasm_eth.wasm e2e/interchaintestv8/wasm
  gzip -n e2e/interchaintestv8/wasm/cw_ics08_wasm_eth.wasm -f

# Build the relayer docker image
# Only for linux/amd64 since sp1 doesn't have an arm image built
[group('build')]
build-relayer-image:
    docker build -t eureka-relayer:latest -f programs/relayer/Dockerfile .

# Install the sp1-ics07-tendermint operator for use in the e2e tests
[group('install')]
install-operator:
	cargo install --bin operator --path programs/operator --locked

# Install the relayer using `cargo install`
[group('install')]
install-relayer:
	cargo install --bin relayer --path programs/relayer --locked

# Run all linters
[group('lint')]
lint:
	@echo "Running all linters..."
	just lint-solidity
	just lint-go
	just lint-buf
	just lint-rust
	just lint-solana

# Lint the Solidity code using `forge fmt` and `bun:solhint`
[group('lint')]
lint-solidity:
	@echo "Linting the Solidity code..."
	forge fmt --check
	bun solhint -w 0 '{scripts,contracts,test}/**/*.sol'
	natlint run --include 'contracts/**/*.sol'

# Lint the Go code using `golangci-lint`
[group('lint')]
lint-go:
	@echo "Linting the Go code..."
	cd e2e/interchaintestv8 && golangci-lint run
	cd packages/go-abigen && golangci-lint run
	cd packages/go-anchor && golangci-lint run

# Lint the Protobuf files using `buf lint`
[group('lint')]
lint-buf:
	@echo "Linting the Protobuf files..."
	buf lint

# Lint the Rust code using `cargo fmt` and `cargo clippy`
[group('lint')]
lint-rust:
	@echo "Linting the Rust code..."
	cargo fmt --all -- --check
	cargo clippy --all-targets --all-features -- -D warnings
	cd programs/sp1-programs && cargo fmt --all -- --check
	cd programs/sp1-programs && cargo clippy --all-targets --all-features -- -D warnings

# Lint the Solana code using `cargo fmt` and `cargo clippy`
[group('lint')]
lint-solana:
	@echo "Linting the Solana code..."
	cd programs/solana && cargo fmt --all -- --check
	cd programs/solana && cargo +nightly clippy --all-targets --all-features -- -D warnings


# Generate the (non-bytecode) ABI files for the contracts
[group('generate')]
generate-abi: build-contracts
	jq '.abi' out/ICS26Router.sol/ICS26Router.json > abi/ICS26Router.json
	jq '.abi' out/ICS20Transfer.sol/ICS20Transfer.json > abi/ICS20Transfer.json
	jq '.abi' out/SP1ICS07Tendermint.sol/SP1ICS07Tendermint.json > abi/SP1ICS07Tendermint.json
	jq '.abi' out/ERC20.sol/ERC20.json > abi/ERC20.json
	jq '.abi' out/IBCERC20.sol/IBCERC20.json > abi/IBCERC20.json
	jq '.abi' out/ICS27Account.sol/ICS27Account.json > abi/ICS27Account.json
	jq '.abi' out/ICS27GMP.sol/ICS27GMP.json > abi/ICS27GMP.json
	jq '.abi' out/RelayerHelper.sol/RelayerHelper.json > abi/RelayerHelper.json
	jq '.abi' out/AttestationLightClient.sol/AttestationLightClient.json > abi/AttestationLightClient.json
	abigen --abi abi/ERC20.json --pkg erc20 --type Contract --out e2e/interchaintestv8/types/erc20/contract.go
	abigen --abi abi/SP1ICS07Tendermint.json --pkg sp1ics07tendermint --type Contract --out packages/go-abigen/sp1ics07tendermint/contract.go
	abigen --abi abi/ICS20Transfer.json --pkg ics20transfer --type Contract --out packages/go-abigen/ics20transfer/contract.go
	abigen --abi abi/ICS26Router.json --pkg ics26router --type Contract --out packages/go-abigen/ics26router/contract.go
	abigen --abi abi/IBCERC20.json --pkg ibcerc20 --type Contract --out packages/go-abigen/ibcerc20/contract.go
	abigen --abi abi/ICS27Account.json --pkg ics27account --type Contract --out packages/go-abigen/ics27account/contract.go
	abigen --abi abi/ICS27GMP.json --pkg ics27gmp --type Contract --out packages/go-abigen/ics27gmp/contract.go
	abigen --abi abi/RelayerHelper.json --pkg relayerhelper --type Contract --out packages/go-abigen/relayerhelper/contract.go
	abigen --abi abi/AttestationLightClient.json --pkg attestation --type Contract --out packages/go-abigen/attestation/contract.go

# Generate the ABI files with bytecode for the required contracts
[group('generate')]
generate-abi-bytecode: build-contracts
	cp out/SP1ICS07Tendermint.sol/SP1ICS07Tendermint.json abi/bytecode
	cp out/AttestationLightClient.sol/AttestationLightClient.json abi/bytecode

# Generate the types for interacting with SVM contracts using 'anchor-go'
[group('generate')]
generate-solana-types: generate-pda
	@echo "Generating SVM types..."
	# Core IBC apps
	rm -rf packages/go-anchor/ics07tendermint
	anchor-go --idl ./programs/solana/target/idl/ics07_tendermint.json --output packages/go-anchor/ics07tendermint --no-go-mod
	rm -rf packages/go-anchor/ics26router
	anchor-go --idl ./programs/solana/target/idl/ics26_router.json --output packages/go-anchor/ics26router --no-go-mod
	rm -rf packages/go-anchor/accessmanager
	anchor-go --idl ./programs/solana/target/idl/access_manager.json --output packages/go-anchor/accessmanager --no-go-mod
	rm -rf packages/go-anchor/ics27gmp
	anchor-go --idl ./programs/solana/target/idl/ics27_gmp.json --output packages/go-anchor/ics27gmp --no-go-mod
	rm -rf packages/go-anchor/ics27ift
	anchor-go --idl ./programs/solana/target/idl/ics27_ift.json --output packages/go-anchor/ics27ift --no-go-mod
	# Dummy apps for testing
	rm -rf e2e/interchaintestv8/solana/go-anchor/dummyibcapp
	anchor-go --idl ./programs/solana/target/idl/dummy_ibc_app.json --output e2e/interchaintestv8/solana/go-anchor/dummyibcapp --no-go-mod
	rm -rf e2e/interchaintestv8/solana/go-anchor/mocklightclient
	anchor-go --idl ./programs/solana/target/idl/mock_light_client.json --output e2e/interchaintestv8/solana/go-anchor/mocklightclient --no-go-mod
	rm -rf e2e/interchaintestv8/solana/go-anchor/gmpcounter
	anchor-go --idl ./programs/solana/target/idl/gmp_counter_app.json --output e2e/interchaintestv8/solana/go-anchor/gmpcounter --no-go-mod
	rm -rf e2e/interchaintestv8/solana/go-anchor/maliciouscaller
	anchor-go --idl ./programs/solana/target/idl/malicious_caller.json --output e2e/interchaintestv8/solana/go-anchor/maliciouscaller --no-go-mod

# Generate Solana PDA helpers from Anchor IDL files
[group('generate')]
generate-pda:
	@echo "Generating Solana PDA helpers from Anchor IDL..."
	go run e2e/interchaintestv8/solana/generate-pdas/main.go \
		--idl-dir programs/solana/target/idl \
		--output e2e/interchaintestv8/solana/pda.go
	@echo "✅ Generated e2e/interchaintestv8/solana/pda.go"

# Generate the fixtures for the wasm tests using the e2e tests
[group('generate')]
generate-fixtures-wasm: clean-foundry install-relayer
	@echo "Generating fixtures... This may take a while."
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 60m
	@echo "Generating native SdkCoin recvPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferNativeCosmosCoinsToEthereumAndBack$' -timeout 60m
	@echo "Generating timeoutPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_TimeoutPacketFromCosmos$' -timeout 60m
	@echo "Generating multi-period client update fixtures..."
	cd e2e/interchaintestv8 && ETH_TESTNET_TYPE=pos GENERATE_WASM_FIXTURES=true go test -v -run '^TestWithRelayerTestSuite/Test_MultiPeriodClientUpdateToCosmos$' -timeout 60m

# Generate the fixtures for the Tendermint light client tests using the e2e tests
[group('generate')]
generate-fixtures-tendermint-light-client: install-relayer
	@echo "Generating Tendermint light client fixtures... This may take a while."
	@echo "Generating basic membership and update client fixtures..."
	cd e2e/interchaintestv8 && GENERATE_TENDERMINT_LIGHT_CLIENT_FIXTURES=true go test -v -run '^TestWithCosmosRelayerTestSuite/Test_UpdateClient$' -timeout 40m

# Generate go types for the e2e tests from the etheruem light client code
[group('generate')]
generate-ethereum-types:
	cargo run --bin generate_json_schema --features test-utils
	bun quicktype --src-lang schema --lang go --just-types-and-package --package ethereum --src ethereum_types_schema.json --out e2e/interchaintestv8/types/ethereum/types.gen.go --top-level GeneratedTypes
	rm ethereum_types_schema.json
	sed -i.bak 's/int64/uint64/g' e2e/interchaintestv8/types/ethereum/types.gen.go # quicktype generates int64 instead of uint64 :(
	rm -f e2e/interchaintestv8/types/ethereum/types.gen.go.bak # this is to be linux and mac compatible (coming from the sed command)
	cd e2e/interchaintestv8 && golangci-lint run --fix types/ethereum/types.gen.go

# Generate the fixtures for the Solidity tests using the e2e tests
[group('generate')]
generate-fixtures-solidity: clean-foundry install-operator install-relayer
	@echo "Generating fixtures... This may take a while."
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket plonk fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures for 25 packets..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_25_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket groth16 fixtures for 50 packets..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_50_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating recvPacket and acknowledgePacket plonk fixtures for 50 packets..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_50_ICS20TransferERC20TokenfromEthereumToCosmosAndBack$' -timeout 40m
	@echo "Generating native SdkCoin recvPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferNativeCosmosCoinsToEthereumAndBack$' -timeout 40m
	@echo "Generating native SdkCoin recvPacket plonk fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_ICS20TransferNativeCosmosCoinsToEthereumAndBack$' -timeout 40m
	@echo "Generating timeoutPacket groth16 fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithIbcEurekaTestSuite/Test_TimeoutPacketFromEth$' -timeout 40m
	@echo "Generating timeoutPacket plonk fixtures..."
	cd e2e/interchaintestv8 && GENERATE_SOLIDITY_FIXTURES=true SP1_PROVER=network E2E_PROOF_TYPE=plonk go test -v -run '^TestWithIbcEurekaTestSuite/Test_TimeoutPacketFromEth$' -timeout 40m

private_cluster := if env("E2E_PRIVATE_CLUSTER", "") == "true" { "--private-cluster" } else { "" }

# Generate the fixture files for `TENDERMINT_RPC_URL` using the prover parameter.
[group('generate')]
generate-fixtures-sp1-ics07: clean-foundry install-operator install-relayer
  @echo "Generating fixtures... This may take a while (up to 20 minutes)"
  TENDERMINT_RPC_URL="${TENDERMINT_RPC_URL%/}" && \
  CURRENT_HEIGHT=$(curl "$TENDERMINT_RPC_URL"/block | jq -r ".result.block.header.height") && \
  TRUSTED_HEIGHT=$(($CURRENT_HEIGHT-100)) && \
  TARGET_HEIGHT=$(($CURRENT_HEIGHT-10)) && \
  echo "For tendermint fixtures, trusted block: $TRUSTED_HEIGHT, target block: $TARGET_HEIGHT, from $TENDERMINT_RPC_URL" && \
  parallel --progress --shebang --ungroup -j 6 ::: \
    "RUST_LOG=info SP1_PROVER=network operator fixtures update-client --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -o 'test/sp1-ics07/fixtures/update_client_fixture-plonk.json' {{private_cluster}}" \
    "sleep 20 && RUST_LOG=info SP1_PROVER=network operator fixtures update-client --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -p groth16 -o 'test/sp1-ics07/fixtures/update_client_fixture-groth16.json' {{private_cluster}}" \
    "sleep 40 && RUST_LOG=info SP1_PROVER=network operator fixtures update-client-and-membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -o 'test/sp1-ics07/fixtures/uc_and_memberships_fixture-plonk.json' {{private_cluster}}" \
    "sleep 60 && RUST_LOG=info SP1_PROVER=network operator fixtures update-client-and-membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT --target-block $TARGET_HEIGHT -p groth16 -o 'test/sp1-ics07/fixtures/uc_and_memberships_fixture-groth16.json' {{private_cluster}}" \
    "sleep 80 && RUST_LOG=info SP1_PROVER=network operator fixtures membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT -o 'test/sp1-ics07/fixtures/memberships_fixture-plonk.json' {{private_cluster}}" \
    "sleep 100 && RUST_LOG=info SP1_PROVER=network operator fixtures membership --key-paths clients/07-tendermint-0/clientState,clients/07-tendermint-001/clientState --trusted-block $TRUSTED_HEIGHT -p groth16 -o 'test/sp1-ics07/fixtures/memberships_fixture-groth16.json' {{private_cluster}}"
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=plonk go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_DoubleSignMisbehaviour$' -timeout 40m
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_BreakingTimeMonotonicityMisbehaviour' -timeout 40m
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=groth16 go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_100_Membership' -timeout 40m
  cd e2e/interchaintestv8 && RUST_LOG=info SP1_PROVER=network GENERATE_SOLIDITY_FIXTURES=true E2E_PROOF_TYPE=plonk go test -v -run '^TestWithSP1ICS07TendermintTestSuite/Test_25_Membership' -timeout 40m
  @echo "Fixtures generated at 'test/sp1-ics07/fixtures'"

# Generate the code from pritibuf using `buf generate`. (Only used for relayer testing at the moment)
[group('generate')]
generate-buf:
    @echo "Generating Protobuf files for relayer"
    buf generate --template buf.gen.yaml

shadowfork := if env("ETH_RPC_URL", "") == "" { "--no-match-path test/shadowfork/*" } else { "" }

# Run all the foundry tests
[group('test')]
test-foundry testname=".\\*":
	forge test -vvv --show-progress --fuzz-runs 5000 --match-test ^{{testname}}\(.\*\)\$ {{shadowfork}}
	@ {{ if shadowfork == "" { "" } else { 'echo ' + BOLD + YELLOW + 'Ran without shadowfork tests since ETH_RPC_URL was not set' } }}

# Run the benchmark tests
[group('test')]
test-benchmark testname=".\\*":
	forge test -vvv --show-progress --gas-report --match-path test/solidity-ibc/BenchmarkTest.t.sol --match-test {{testname}}

# Run the cargo tests
[group('test')]
test-cargo testname="--all":
	cargo test {{testname}} --locked --no-fail-fast -- --nocapture

# Run the tests in abigen
[group('test')]
test-abigen:
	@echo "Running abigen tests..."
	cd packages/go-abigen && go test -v ./...

# Run any e2e test using the test's full name. For example, `just test-e2e TestWithIbcEurekaTestSuite/Test_Deploy`
[group('test')]
test-e2e testname: clean-foundry install-relayer
	@echo "Running {{testname}} test..."
	cd e2e/interchaintestv8 && go test -v -run '^{{testname}}$' -timeout 120m

# Run any e2e test in the IbcEurekaTestSuite. For example, `just test-e2e-eureka Test_Deploy`
[group('test')]
test-e2e-eureka testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithIbcEurekaTestSuite/{{testname}}

# Run any e2e test in the RelayerTestSuite. For example, `just test-e2e-relayer Test_RelayerInfo`
[group('test')]
test-e2e-relayer testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithRelayerTestSuite/{{testname}}

# Run any e2e test in the CosmosRelayerTestSuite. For example, `just test-e2e-cosmos-relayer Test_RelayerInfo`
[group('test')]
test-e2e-cosmos-relayer testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithCosmosRelayerTestSuite/{{testname}}

# Run anu e2e test in the SP1ICS07TendermintTestSuite. For example, `just test-e2e-sp1-ics07 Test_Deploy`
[group('test')]
test-e2e-sp1-ics07 testname: install-operator
	@echo "Running {{testname}} test..."
	just test-e2e TestWithSP1ICS07TendermintTestSuite/{{testname}}

# Run any e2e test in the MultichainTestSuite. For example, `just test-e2e-multichain Test_Deploy`
[group('test')]
test-e2e-multichain testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithMultichainTestSuite/{{testname}}

# Run the e2e tests in the IbcEurekaSolanaTestSuite. For example, `just test-e2e-solana Test_Deploy`
[group('test')]
test-e2e-solana testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithIbcEurekaSolanaTestSuite/{{testname}}

# Run the e2e tests in the IbcEurekaSolanaGMPTestSuite. For example, `just test-e2e-solana-gmp Test_GMPSPLTokenTransferFromCosmos`
[group('test')]
test-e2e-solana-gmp testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithIbcEurekaSolanaGMPTestSuite/{{testname}}

# Run the e2e tests in the IbcEurekaSolanaUpgradeTestSuite. For example, `just test-e2e-solana-upgrade Test_ProgramUpgrade_Via_AccessManager`
[group('test')]
test-e2e-solana-upgrade testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithIbcEurekaSolanaUpgradeTestSuite/{{testname}}

# Run the Solana Anchor e2e tests
[group('test')]
test-anchor-solana *ARGS:
	@echo "Running Solana Client Anchor tests..."
	@echo "🦀 Using {{anchor_cmd}}"
	(cd programs/solana && {{anchor_cmd}} test {{ARGS}})

# Run Solana unit tests (mollusk + litesvm)
[group('test')]
test-solana *ARGS:
	@echo "Building and running Solana unit tests..."
	@echo "🦀 Using {{anchor_cmd}}"
	@if [ "{{anchor_cmd}}" = "anchor-nix" ]; then \
		(cd programs/solana && anchor-nix unit-test {{ARGS}}); \
	else \
		(cd programs/solana && anchor build) && \
		echo "✅ Build successful, running cargo tests" && \
		(cd programs/solana && cargo test {{ARGS}}); \
	fi

# Run any e2e test in the IbcEurekaGmpTestSuite. For example, `just test-e2e-multichain TestDeploy_Groth16`
[group('test')]
test-e2e-gmp testname:
	@echo "Running {{testname}} test..."
	just test-e2e TestWithIbcEurekaGmpTestSuite/{{testname}}

# Clean up the foundry cache and out directories
[group('clean')]
clean-foundry:
	@echo "Cleaning up cache and out directories"
	-rm -rf cache out broadcast # ignore errors

# Clean up the cargo artifacts using `cargo clean`
[group('clean')]
clean-cargo:
	@echo "Cleaning up cargo target directory"
	cargo clean
	cd programs/sp1-programs && cargo clean

# Run Slither static analysis on contracts
[group('security')]
slither:
	@echo "Running Slither static analysis..."
	slither . --config-file .slither.config.json
