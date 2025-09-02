#!/bin/sh

set -e

# Directory where binaries are installed
BIN_DIR="/usr/local/bin"

print_help() {
    cat << EOF
Usage: <COMMAND> [ARGS...]

Commands:
    help                    Show this help message
    sh                      Start an interactive shell
    relayer                 Run the relayer with [ARGS...]
    attestor-optimism       Run the optimism attestor with [ARGS...]
    attestor-arbitrum       Run the arbitrum attestor with [ARGS...]
    attestor-cosmos         Run the cosmos attestor with [ARGS...]

Examples:
    docker run --rm ibc-eureka:latest relayer --config config.json
    docker run --rm ibc-eureka:latest attestor-optimism --help
    docker run --rm ibc-eureka:latest sh

EOF
}

if [ $# -eq 0 ]; then
    echo "Error: No command provided"
    echo ""
    print_help
    exit 1
fi

COMMAND="$1"
shift

case "$COMMAND" in
    help|--help|-h)
        print_help
        exit 0
        ;;
    sh)
        exec sh
        ;;
    relayer)
        exec "$BIN_DIR/relayer" "$@"
        ;;
    attestor-optimism)
        exec "$BIN_DIR/attestor-optimism" "$@"
        ;;
    attestor-arbitrum)
        exec "$BIN_DIR/attestor-arbitrum" "$@"
        ;;
    attestor-cosmos)
        exec "$BIN_DIR/attestor-cosmos" "$@"
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        echo ""
        print_help
        exit 1
        ;;
esac
