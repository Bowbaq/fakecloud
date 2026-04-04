#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SERVER_BIN="$REPO_ROOT/target/release/fakecloud-server"
SERVER_PID=""

cleanup() {
    if [ -n "$SERVER_PID" ] && kill -0 "$SERVER_PID" 2>/dev/null; then
        echo "Stopping FakeCloud server (PID $SERVER_PID)..."
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "=== Building FakeCloud ==="
cd "$REPO_ROOT"
cargo build --release 2>&1

echo ""
echo "=== Starting FakeCloud server ==="
"$SERVER_BIN" --log-level warn &
SERVER_PID=$!
echo "Server PID: $SERVER_PID"

# Wait for server to be ready
for i in $(seq 1 30); do
    if curl -s http://localhost:4566/ >/dev/null 2>&1; then
        echo "Server is ready."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "ERROR: Server did not start within 30 seconds."
        exit 1
    fi
    sleep 1
done

echo ""
echo "=== Running compatibility tests ==="
python3 "$SCRIPT_DIR/boto3_compat.py"
EXIT_CODE=$?

echo ""
echo "=== Done ==="
exit $EXIT_CODE
