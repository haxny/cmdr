#!/bin/bash
# Stop the NFS fixture server.
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_NAME="nfs-fixture"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

# Release the "manual" lease. The helper downs the stack ONLY at zero holders, so
# running this while a sibling worktree's suite is live leaves that stack up.
echo "Releasing the manual NFS lease (the stack downs only at zero holders)..."
released=false
if command -v go &> /dev/null; then
    if (cd "$REPO_ROOT/scripts/check" && go run ./stack-lease release nfs manual); then
        released=true
    else
        echo "WARN: NFS lease helper failed; falling back to a direct 'compose down'." >&2
    fi
else
    echo "WARN: 'go' not found; falling back to a direct 'compose down' (no cross-worktree refcounting)." >&2
fi

if [ "$released" = false ]; then
    docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" down
fi

echo "Done."
