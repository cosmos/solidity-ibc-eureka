#!/usr/bin/env bash
#
# stream-simd-logs.sh
#
# Description:
#   Waits for Docker containers whose names start with `simd-`, then streams
#   their logs into timestamped files in a descriptive subdirectory under
#   ../tmp/logs. Each run gets its own directory, e.g.:
#       ../tmp/logs/run_20251014_132500_descriptive
#   Supports Ctrl+C to terminate all background log streams cleanly.
#

set -e

# Prefix for this run's directory (customize if desired)
RUN_PREFIX="run"

# Create a timestamp for this run
timestamp=$(date +"%Y%m%d_%H%M%S")

# Optional descriptive name for this run (can be empty)
DESCRIPTIVE_NAME="simd-logs"

# Directory of the script
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Logs directory for this run
log_dir="${script_dir}/../tmp/logs/${RUN_PREFIX}_${timestamp}_${DESCRIPTIVE_NAME}"
mkdir -p "$log_dir"

# Track background docker logs PIDs
pids=()

# Trap Ctrl+C / termination to stop all background logs
cleanup() {
    echo
    echo "Stopping all log streams..."
    for pid in "${pids[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    wait
    echo "All background log processes stopped."
    exit
}
trap cleanup INT TERM

# Wait for containers matching simd-* to appear
echo "Waiting for containers matching pattern 'simd-*' to start..."
while true; do
    containers=$(docker ps --format '{{.Names}}' | grep '^simd-' || true)
    if [ -n "$containers" ]; then
        echo "Found containers:"
        echo "$containers"
        break
    fi
    echo "Still waiting for containers..."
    sleep 2
done

# Attach logs for each container
for name in $containers; do
    log_file="${log_dir}/${name}.log"
    echo "Attaching logs for $name ..."

    # Wait until docker logs works (container ready)
    until docker logs "$name" > /dev/null 2>&1; do
        sleep 0.5
    done

    echo "→ Writing logs to: $log_file"
    (docker logs -f "$name" > "$log_file" 2>&1) &
    pids+=($!)
done

echo
echo "Log streaming started for all containers."
echo "Logs for this run are in: $log_dir/"
echo

# Wait for all background log streams to finish
wait "${pids[@]}"
echo "All log streams have finished."
