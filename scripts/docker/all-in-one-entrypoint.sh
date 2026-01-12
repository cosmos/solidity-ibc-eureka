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

Examples:
    docker run --rm ibc-eureka:latest relayer --config config.json
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
        exec /busybox/sh
        ;;
    relayer)
        exec "$BIN_DIR/relayer" "$@"
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        echo ""
        print_help
        exit 1
        ;;
esac
