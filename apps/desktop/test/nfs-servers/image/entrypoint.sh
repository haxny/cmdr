#!/bin/sh
# Brings up the kernel NFS server (knfsd) inside this container's network
# namespace: rpcbind, the exports, `rpc.mountd`, then `rpc.nfsd`.
#
# ❗ `rpc.nfsd` and `rpc.mountd` listen on the SAME port numbers the compose
# file publishes on the host (`NFS_PORT`, `MOUNT_PORT`). portmap answers the
# ports a service registered INSIDE the container, so a client on the host only
# gets a usable answer from `GETPORT` when the two numbers are equal. rpcbind
# itself stays on 111 and the host reaches it through a different published port,
# which is why a client takes the portmap port from its params (the README,
# § "The portmap port trick").
set -e

NFS_PORT="${NFS_PORT:?the compose file sets NFS_PORT}"
MOUNT_PORT="${MOUNT_PORT:?the compose file sets MOUNT_PORT}"
NFSD_THREADS="${NFSD_THREADS:-8}"

if ! grep -qw nfsd /proc/filesystems; then
    # On Linux the host kernel builds `nfsd` as a module, and a container can't
    # load one (no `/lib/modules` in here). CI runs `sudo modprobe nfsd` before
    # the lane; a dev Linux box needs the same once per boot.
    echo "nfs-fixture: the host kernel has no nfsd. Run 'sudo modprobe nfsd' on the Docker host." >&2
    exit 1
fi
if ! mountpoint -q /proc/fs/nfsd; then
    mount -t nfsd nfsd /proc/fs/nfsd
fi

# ── The exports ──────────────────────────────────────────────────────
#
# Re-seeded from scratch on every container start, so a cell that wrote into
# `/insecure` last run leaves nothing behind for this one. ❗ Both are named
# volumes, ❌ never the image layer: knfsd refuses to export an overlayfs
# directory (`does not support NFS export`).
rm -f /run/nfs-fixture-ready
for export in /secure /insecure; do
    find "$export" -mindepth 1 -delete
done
/usr/local/bin/seed.sh /secure
cp -a /secure/. /insecure/
# Restored after the copy: `cp -a` keeps owners and modes but the export root
# is the volume's own mountpoint, which it doesn't touch.
chown 0:0 /insecure
chmod 0777 /insecure

# `secure` is knfsd's default and spelled out anyway: it's the whole point of
# the export. knfsd checks it on EVERY NFS call (answering `AUTH_TOOWEAK` past
# port 1023), and `rpc.mountd` checks it on MNT.
#
# `sync` makes the server honour the client's `stable_how`, so an UNSTABLE
# WRITE answers UNSTABLE and the COMMIT verifier path is real. Under `async`
# knfsd answers every write as if it were stable without making it so.
#
# `fsid=` because a named volume's filesystem UUID isn't something the exports
# should depend on; ❌ never `fsid=0`, which is NFSv4's pseudo-root.
cat > /etc/exports <<'EXPORTS'
/secure   *(rw,sync,no_subtree_check,secure,root_squash,fsid=1)
/insecure *(rw,sync,no_subtree_check,insecure,root_squash,fsid=2)
EXPORTS

# ── The daemons ──────────────────────────────────────────────────────
mkdir -p /run/rpcbind /var/lib/nfs/rpc_pipefs /var/lib/nfs/v4recovery
rpcbind -w

exportfs -ra

# v3 only: NFSv4 needs an id mapper and a recovery tracker this fixture has no
# use for, and the client speaks v3 alone.
rpc.mountd --port "$MOUNT_PORT" --no-nfs-version 4 --no-udp
rpc.nfsd --port "$NFS_PORT" --no-nfs-version 4 --nfs-version 3 --tcp --no-udp "$NFSD_THREADS"

stop() {
    # ❗ Stop the kernel threads before the container goes: knfsd lives in the
    # host kernel, so a container that exits with them running leaves them to
    # the namespace teardown rather than to us.
    rpc.nfsd 0 || true
    exportfs -au || true
    killall rpc.mountd rpcbind 2>/dev/null || true
    exit 0
}
trap stop TERM INT

touch /run/nfs-fixture-ready
echo "nfs-fixture: knfsd up (nfs ${NFS_PORT}/tcp, mount ${MOUNT_PORT}/tcp, portmap 111/tcp)"
rpcinfo -p 127.0.0.1

tail -f /dev/null &
wait $!
