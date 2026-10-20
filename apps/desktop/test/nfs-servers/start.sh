#!/bin/bash
# Start the NFS fixture server (knfsd) for local development and the
# integration lane.
#
# First-party, like the S3 stack next door: the compose file sits right here.
#
# Usage:
#   ./start.sh           # core: the knfsd server the integration lane talks to
#   ./start.sh minimal   # the same server (the stack has one)
#   ./start.sh all       # everything the compose file defines

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_NAME="nfs-fixture"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

# The same defaults the compose file and `nfsProgramHostPorts` carry
# (`TestNfsFixturePortsMatchComposeDefaults`).
PORTMAP_PORT="${NFS_FIXTURE_PORTMAP_PORT:-15480}"
NFS_PORT="${NFS_FIXTURE_NFS_PORT:-15481}"
MOUNT_PORT="${NFS_FIXTURE_MOUNT_PORT:-15482}"

# ❗ Keep this table in lock-step with `modeServices` in
# scripts/check/stacklease/registry.go (`TestNfsModeServicesAgree`).
mode="${1:-core}"
services=()

case "$mode" in
    minimal)
        echo "Starting the minimal NFS server (knfsd)..."
        services=(nfs-fixture-knfsd)
        ;;
    core)
        echo "Starting the core NFS server (knfsd, exporting /secure and /insecure)..."
        services=(nfs-fixture-knfsd)
        ;;
    all)
        echo "Starting every NFS server the compose file defines..."
        ;;
    *)
        echo "Unknown mode: $mode"
        echo "Usage: $0 [minimal|core|all]"
        exit 1
        ;;
esac

# Adopt-or-start through the machine-wide NFS lease, so a sibling worktree's
# live suite is never recreated or torn down under it. Same model as the other
# fixture stacks, its own lease namespace: scripts/check/stacklease.
#
# A bare `start.sh` registers as the "manual" sentinel holder that the dead-PID
# sweep never reaps; `stop.sh` is what clears it.
lease_ok=false
if command -v go &> /dev/null; then
    if (cd "$REPO_ROOT/scripts/check" && go run ./stack-lease acquire nfs manual "$mode"); then
        lease_ok=true
    else
        echo "WARN: NFS lease helper failed; falling back to a direct 'compose up' (no cross-worktree refcounting)." >&2
    fi
else
    echo "WARN: 'go' not found; falling back to a direct 'compose up' (no cross-worktree NFS lease refcounting)." >&2
fi

if [ "$lease_ok" = false ]; then
    docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" up -d --build "${services[@]}"
fi

if [ ${#services[@]} -eq 0 ]; then
    # A `read` loop, not `mapfile`: macOS's /bin/bash is 3.2, which has none.
    while IFS= read -r service; do
        services+=("$service")
    done < <(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" ps --services)
fi

# One RPC `NULL` call over TCP from this host, through a published port: proof
# that the publish, the record marking, and the program/version registration all
# line up, which the container's own HEALTHCHECK (run inside) can't show. Perl
# because macOS has no `showmount`/`rpcinfo` that takes a non-standard port over
# TCP reliably, and Perl ships on both macOS and the Ubuntu runner. `alarm`
# bounds it, so a wedged publish fails here rather than hanging the run.
rpc_null() {
    perl - "$1" "$2" "$3" <<'PROBE'
use strict;
use IO::Socket::INET;
my ($port, $prog, $vers) = @ARGV;
alarm 5;
my $s = IO::Socket::INET->new(PeerAddr => "127.0.0.1", PeerPort => $port, Proto => "tcp") or exit 1;
my $xid = int(rand(2**31));
# xid, CALL, rpcvers 2, prog, vers, proc NULL, AUTH_NONE cred, AUTH_NONE verf.
my $call = pack("N*", $xid, 0, 2, $prog, $vers, 0, 0, 0, 0, 0);
print $s pack("N", 0x80000000 | length($call)) . $call;
my $got = "";
while (length($got) < 28) {
    my $n = sysread($s, $got, 28 - length($got), length($got));
    exit 1 unless $n;
}
# record mark, xid, REPLY, MSG_ACCEPTED, verf flavor, verf length, SUCCESS.
my (undef, $rxid, $type, $stat, undef, undef, $accept) = unpack("N7", $got);
exit(($rxid == $xid && $type == 1 && $stat == 0 && $accept == 0) ? 0 : 1);
PROBE
}

# Readiness is the container's HEALTHCHECK first (portmap lists MOUNT v3 and
# NFS v3, and the seed is done; a bound port comes up long before that), then
# the three host-side NULL calls. ❌ Never a `sleep N` (see ../CLAUDE.md, "No
# magic timer waits").
echo ""
echo "Waiting for knfsd to seed its exports and answer RPC from this host..."
for service in "${services[@]}"; do
    deadline=$((SECONDS + 120))
    until [ "$(docker inspect -f '{{.State.Health.Status}}' "$(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" ps -q "$service")" 2>/dev/null)" = "healthy" ]; do
        if [ $SECONDS -ge $deadline ]; then
            echo "ERROR: $service didn't report healthy within 120s" >&2
            docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" logs --tail=50 "$service" >&2
            exit 1
        fi
        sleep 0.2
    done
done

for probe in "portmap $PORTMAP_PORT 100000 2" "MOUNT $MOUNT_PORT 100005 3" "NFS $NFS_PORT 100003 3"; do
    set -- $probe
    if ! rpc_null "$2" "$3" "$4"; then
        echo "ERROR: knfsd is healthy, but $1 v$4 didn't answer a NULL call on 127.0.0.1:$2" >&2
        exit 1
    fi
    echo "  ✓ $1 v$4 answers on 127.0.0.1:$2"
done

echo ""
echo "NFS server ready. Exports /secure and /insecure; portmap on 127.0.0.1:$PORTMAP_PORT."
echo "  ❗ /secure refuses every client reaching it through a published port (the source port is"
echo "     rewritten past 1023). The README, § \"Reaching /secure\"."
echo ""
echo "Stop it with './apps/desktop/test/nfs-servers/stop.sh'."
