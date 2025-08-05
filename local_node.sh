#!/bin/bash

# Local Node Setup Script for Cosmos-EVM
# This script sets up and runs a local Cosmos-EVM node for development and testing

set -e  # Exit on error
set -u  # Exit on undefined variable

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
CHAIN_ID="${CHAIN_ID:-evmos_9000-1}"
MONIKER="${MONIKER:-local-node}"
KEYRING_BACKEND="${KEYRING_BACKEND:-test}"
NODE_HOME="${NODE_HOME:-$HOME/.evmos}"
BINARY_NAME="${BINARY_NAME:-evmosd}"
DENOM="${DENOM:-aevmos}"
RPC_PORT="${RPC_PORT:-26657}"
P2P_PORT="${P2P_PORT:-26656}"
GRPC_PORT="${GRPC_PORT:-9090}"
GRPC_WEB_PORT="${GRPC_WEB_PORT:-9091}"
JSON_RPC_PORT="${JSON_RPC_PORT:-8545}"
WS_PORT="${WS_PORT:-8546}"
API_PORT="${API_PORT:-1317}"

# Function to print colored output
print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to check if port is available
is_port_available() {
    ! lsof -i:$1 >/dev/null 2>&1
}

# Function to wait for port to be available
wait_for_port() {
    local port=$1
    local timeout=${2:-30}
    local counter=0
    
    while ! is_port_available $port; do
        if [ $counter -ge $timeout ]; then
            print_error "Port $port is still in use after $timeout seconds"
            return 1
        fi
        print_warning "Port $port is in use, waiting..."
        sleep 1
        ((counter++))
    done
    return 0
}

# Function to cleanup on exit
cleanup() {
    print_info "Cleaning up..."
    if [ -n "${NODE_PID:-}" ] && kill -0 $NODE_PID 2>/dev/null; then
        print_info "Stopping node process (PID: $NODE_PID)"
        kill $NODE_PID 2>/dev/null || true
        sleep 2
        kill -9 $NODE_PID 2>/dev/null || true
    fi
}

# Set trap for cleanup
trap cleanup EXIT INT TERM

# Main setup function
main() {
    print_info "Starting Cosmos-EVM local node setup..."
    print_info "Chain ID: $CHAIN_ID"
    print_info "Moniker: $MONIKER"
    print_info "Node Home: $NODE_HOME"
    
    # Check dependencies
    print_info "Checking dependencies..."
    
    if ! command_exists go; then
        print_error "Go is not installed. Please install Go 1.19 or later."
        exit 1
    fi
    
    if ! command_exists jq; then
        print_error "jq is not installed. Please install jq."
        exit 1
    fi
    
    # Check if binary exists
    if ! command_exists $BINARY_NAME; then
        print_warning "$BINARY_NAME not found in PATH. Attempting to build..."
        
        # Try to build the binary
        if [ -f "Makefile" ]; then
            print_info "Building $BINARY_NAME..."
            make install || {
                print_error "Failed to build $BINARY_NAME"
                exit 1
            }
        else
            print_error "$BINARY_NAME not found and no Makefile present"
            exit 1
        fi
    fi
    
    # Check ports availability
    print_info "Checking port availability..."
    local ports=($RPC_PORT $P2P_PORT $GRPC_PORT $GRPC_WEB_PORT $JSON_RPC_PORT $WS_PORT $API_PORT)
    for port in "${ports[@]}"; do
        if ! is_port_available $port; then
            print_warning "Port $port is already in use"
            wait_for_port $port 5 || {
                print_error "Cannot proceed, port $port is in use"
                exit 1
            }
        fi
    done
    
    # Initialize node if not already initialized
    if [ ! -d "$NODE_HOME/config" ]; then
        print_info "Initializing node..."
        $BINARY_NAME init $MONIKER --chain-id $CHAIN_ID --home $NODE_HOME
    else
        print_info "Node already initialized at $NODE_HOME"
    fi
    
    # Configure node
    print_info "Configuring node..."
    
    # Update config.toml
    CONFIG_TOML="$NODE_HOME/config/config.toml"
    if [ -f "$CONFIG_TOML" ]; then
        # Enable API
        sed -i 's/enable = false/enable = true/g' "$NODE_HOME/config/app.toml"
        
        # Update ports in config
        sed -i "s|laddr = \"tcp://127.0.0.1:26657\"|laddr = \"tcp://0.0.0.0:$RPC_PORT\"|g" "$CONFIG_TOML"
        sed -i "s|laddr = \"tcp://0.0.0.0:26656\"|laddr = \"tcp://0.0.0.0:$P2P_PORT\"|g" "$CONFIG_TOML"
        
        # Enable unsafe RPC for development
        sed -i 's/unsafe = false/unsafe = true/g' "$CONFIG_TOML"
        
        # Reduce block time for faster development
        sed -i 's/timeout_commit = "5s"/timeout_commit = "1s"/g' "$CONFIG_TOML"
    fi
    
    # Update app.toml
    APP_TOML="$NODE_HOME/config/app.toml"
    if [ -f "$APP_TOML" ]; then
        # Configure EVM RPC
        sed -i "s|address = \"127.0.0.1:8545\"|address = \"0.0.0.0:$JSON_RPC_PORT\"|g" "$APP_TOML"
        sed -i "s|ws-address = \"127.0.0.1:8546\"|ws-address = \"0.0.0.0:$WS_PORT\"|g" "$APP_TOML"
        
        # Configure gRPC
        sed -i "s|address = \"0.0.0.0:9090\"|address = \"0.0.0.0:$GRPC_PORT\"|g" "$APP_TOML"
        sed -i "s|address = \"0.0.0.0:9091\"|address = \"0.0.0.0:$GRPC_WEB_PORT\"|g" "$APP_TOML"
        
        # Configure API
        sed -i "s|address = \"tcp://0.0.0.0:1317\"|address = \"tcp://0.0.0.0:$API_PORT\"|g" "$APP_TOML"
        
        # Set minimum gas price
        sed -i "s|minimum-gas-prices = \"\"|minimum-gas-prices = \"0$DENOM\"|g" "$APP_TOML"
    fi
    
    # Create or import test keys
    print_info "Setting up test accounts..."
    
    # Check if key exists
    if ! $BINARY_NAME keys show validator --keyring-backend $KEYRING_BACKEND --home $NODE_HOME >/dev/null 2>&1; then
        print_info "Creating validator key..."
        $BINARY_NAME keys add validator --keyring-backend $KEYRING_BACKEND --home $NODE_HOME
    fi
    
    # Add genesis account if needed
    if ! grep -q "validator" "$NODE_HOME/config/genesis.json"; then
        print_info "Adding validator to genesis..."
        VALIDATOR_ADDR=$($BINARY_NAME keys show validator -a --keyring-backend $KEYRING_BACKEND --home $NODE_HOME)
        $BINARY_NAME add-genesis-account $VALIDATOR_ADDR 100000000000000000000000$DENOM --keyring-backend $KEYRING_BACKEND --home $NODE_HOME
        
        # Create genesis transaction
        print_info "Creating genesis transaction..."
        $BINARY_NAME gentx validator 1000000000000000000000$DENOM \
            --keyring-backend $KEYRING_BACKEND \
            --chain-id $CHAIN_ID \
            --home $NODE_HOME
        
        # Collect genesis transactions
        $BINARY_NAME collect-gentxs --home $NODE_HOME
    fi
    
    # Validate genesis
    print_info "Validating genesis..."
    $BINARY_NAME validate-genesis --home $NODE_HOME
    
    # Start the node
    print_info "Starting node..."
    print_info "RPC endpoint: http://localhost:$RPC_PORT"
    print_info "EVM RPC endpoint: http://localhost:$JSON_RPC_PORT"
    print_info "gRPC endpoint: localhost:$GRPC_PORT"
    print_info "API endpoint: http://localhost:$API_PORT"
    
    # Start node in background
    $BINARY_NAME start --home $NODE_HOME \
        --json-rpc.enable=true \
        --json-rpc.address="0.0.0.0:$JSON_RPC_PORT" \
        --json-rpc.ws-address="0.0.0.0:$WS_PORT" &
    
    NODE_PID=$!
    print_info "Node started with PID: $NODE_PID"
    
    # Wait for node to be ready
    print_info "Waiting for node to be ready..."
    local counter=0
    while ! curl -s http://localhost:$RPC_PORT/status >/dev/null 2>&1; do
        if [ $counter -ge 30 ]; then
            print_error "Node failed to start within 30 seconds"
            exit 1
        fi
        sleep 1
        ((counter++))
    done
    
    print_info "Node is ready!"
    
    # Print useful information
    print_info "Node Information:"
    echo "  Chain ID: $CHAIN_ID"
    echo "  Node ID: $($BINARY_NAME tendermint show-node-id --home $NODE_HOME)"
    echo "  Validator Address: $($BINARY_NAME keys show validator -a --keyring-backend $KEYRING_BACKEND --home $NODE_HOME)"
    
    # Keep script running
    print_info "Node is running. Press Ctrl+C to stop."
    wait $NODE_PID
}

# Show usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  -h, --help              Show this help message"
    echo "  -c, --chain-id ID       Set chain ID (default: $CHAIN_ID)"
    echo "  -m, --moniker NAME      Set moniker (default: $MONIKER)"
    echo "  -d, --home DIR          Set home directory (default: $NODE_HOME)"
    echo ""
    echo "Environment variables:"
    echo "  CHAIN_ID                Chain ID"
    echo "  MONIKER                 Node moniker"
    echo "  NODE_HOME               Node home directory"
    echo "  KEYRING_BACKEND         Keyring backend (default: test)"
    echo "  RPC_PORT                RPC port (default: 26657)"
    echo "  JSON_RPC_PORT           EVM JSON-RPC port (default: 8545)"
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -c|--chain-id)
            CHAIN_ID="$2"
            shift 2
            ;;
        -m|--moniker)
            MONIKER="$2"
            shift 2
            ;;
        -d|--home)
            NODE_HOME="$2"
            shift 2
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Run main function
main