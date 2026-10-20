# The NFS fixture stack

One real Linux kernel NFS server (knfsd) in Docker, the server every QNAP, Synology, and TrueNAS runs, exporting
`/secure` and `/insecure`. `crates/cmdr-nfs`'s Docker cells and the app's NFS suites both talk to it.

```bash
./start.sh          # core: the one server the integration lane uses
./stop.sh           # releases this shell's lease; downs only at zero holders
```

`pnpm check` brings the stack up on its own (`desktop-rust-integration-tests` declares `NfsCore`), so a manual
`start.sh` is for iterating by hand. `start.sh` waits for the container's health check, then makes one RPC `NULL` call
per program from the host through each published port.

❌ **Never pause, stop, or kill the container to simulate a server going away**: other test binaries, worktrees, and
sessions lease the same stack at the same time. Put a `cmdr_fs::testing::tcp_proxy::TcpProxy` between the client and the
fixture and cut that.

## Ports and what the cells read

Every port publishes on `${NFS_BIND_ADDR:-127.0.0.1}`, in this stack's own range, 15480+ (S3 owns 14480+, WebDAV 13480+,
SFTP 12480+, SMB 11480+). The check runner exports each variable before bring-up (`ApplyNfsPortEnv`); a cell reads the
same variable with the same default.

- **`NFS_FIXTURE_PORTMAP_PORT`, 15480**: rpcbind, published onto the container's 111.
- **`NFS_FIXTURE_NFS_PORT`, 15481**: `rpc.nfsd`, NFS v3 over TCP, published on the same number inside and out.
- **`NFS_FIXTURE_MOUNT_PORT`, 15482**: `rpc.mountd`, MOUNT v3 over TCP, same number inside and out.
- **`NFS_FIXTURE_DIRECT_HOST`**: the container's own IP, set by the integration lane **on Linux only**, and only when it
  answers TCP. Unset everywhere else. The one way to `/secure`; see § "Reaching /secure".

A crate cell sets `portmap_port` (and, if it wants to skip a lookup, `mount_port`) on `NfsConnectionParams` from these;
an `nfs://` URL's port is the NFS port and never the portmap one.

## The portmap port trick

portmap answers the port a program registered INSIDE the container. If `rpc.nfsd` listened on 2049 and the host reached
it through 15481, every `GETPORT` would answer 2049, a port that means nothing on the host. So the two daemons listen on
the exact numbers the host publishes (`15481:15481`, the same variable on both sides and in the `NFS_PORT` /
`MOUNT_PORT` env the entrypoint reads), and only rpcbind itself moves. A client that takes the portmap port from its
params then follows every `GETPORT` answer as it stands. `TestNfsFixturePublishesEachProgramOnItsOwnNumber` fails the
run if a mapping or the env stops agreeing.

On the direct path (`NFS_FIXTURE_DIRECT_HOST`), rpcbind is on its real 111 and the same `GETPORT` answers are the
container's real ports, so a cell there needs no overrides at all.

## The exports

Both exports carry the same tree, written by `image/seed.sh` and wiped and rewritten on every container start (so a cell
that wrote into `/insecure` last run leaves nothing for this one):

- `hello.txt` (owned by 0, mode 0644), `ten-bytes.txt`, `five-bytes.txt`, `zero-bytes.txt`, `dated.txt` (2021-01-29
  08:30:15 UTC, `conformance::SOURCE_DATE_SECS`).
- `photos/2024 summer/beach.txt`, `nested/deep/deeper/file.txt`, `empty-dir/`.
- `a file with spaces.txt`, and `café.txt` twice: composed (U+00E9) and decomposed (`e` + U+0301). Two files on a Linux
  server, which stores bytes.
- `link-inside` → `photos/2024 summer/beach.txt` (relative, stays in the export) and `link-outside` → `/etc/hostname`
  (absolute, leaves it).
- `oddnames/caf\xE9.txt` (latin-1, not valid UTF-8) beside `oddnames/fine.txt`.
- `large.bin`, 200 MiB (`LARGE_MB`), the same self-describing format as the SFTP and WebDAV fixtures: every 16-byte line
  holds its own zero-padded line number, so a cell checks byte-exactness without shipping a copy.

Every entry belongs to uid/gid 1000 except `hello.txt`; the export root is root's with mode 0777. That's a QNAP share's
shape, and it's what `guess_identity` must answer 1000 for (the most common non-zero owner), ❌ not 0 from the root.

`/etc/exports`: both `rw,sync,no_subtree_check,root_squash`, `/secure` with `secure` (knfsd's default) and `fsid=1`,
`/insecure` with `insecure` and `fsid=2`. `sync` makes knfsd honour the client's `stable_how`, so an UNSTABLE WRITE
answers UNSTABLE and the COMMIT verifier path is real; under `async` knfsd answers every write as stable without making
it so. Both live on named volumes, ❌ never the image layer: knfsd refuses to export an overlayfs directory.

## Why knfsd

The plan's first candidate was unfs3, a user-space server that needs no privilege. It's a partial stand-in for a NAS
(read in its source, `nfs.c`, `mount.c`, `unfsd.8` on `master`, 2026-10-20): it answers READDIRPLUS with
`NFS3ERR_NOTSUPP`, checks `secure` on MNT only, and ignores everything but the mode in CREATE's attributes. knfsd is the
real thing, so it's the only server here. The READDIR fallback is `fake_nfs.rs`'s job.

What it costs, and why it's paid:

- **`privileged: true`.** The server is the host kernel's `nfsd`: the entrypoint mounts `/proc/fs/nfsd` and `rpc.nfsd`
  hands the kernel its listening sockets, which `CAP_SYS_ADMIN` alone doesn't get past Ubuntu's AppArmor
  `docker-default` profile. knfsd is per network namespace, so the container's server is its own and nothing reaches the
  VM's or the runner's.
- **The host kernel must have `nfsd`.** OrbStack's kernel builds it in (verified on OrbStack 2.2.4, kernel
  `7.2.9-orbstack`, `/proc/filesystems` lists `nfsd`, 2026-10-20). Ubuntu builds it as a module a container can't load,
  so CI runs `sudo modprobe nfsd` first (`.github/workflows/ci.yml`, "Prepare the host for the NFS fixture"), and so
  must a Linux dev box, once per boot. The entrypoint fails loudly naming the fix when it's missing. ⚠️ The GitHub
  runner half is reasoned, not yet observed: the first CI run of this lane is the measurement.
- **The userland is Alpine 3.24's `nfs-utils` 2.6.4 and `rpcbind` 1.2.9**, the canonical Linux NFS tools (upstream at
  `git.kernel.org/pub/scm/linux/kernel/git/cel/nfs-utils`), pinned by the image digest.
- **The container stops its kernel threads on the way out** (`rpc.nfsd 0` in the `TERM` trap), so a restart takes under
  a second and a re-seed starts clean.

v4 is switched off (`/proc/fs/nfsd/versions` reads `+3 -4 -4.0 -4.1 -4.2`): a v4 `NULL` answers `PROG_MISMATCH`. `lockd`
still registers `nlockmgr` on random ports; nothing here uses it.

## Reaching /secure

`/secure` refuses any client whose source port is past 1023, and a published port rewrites the source port. Measured
2026-10-20 on this box (OrbStack 2.2.4, macOS 27.0, kernel `7.2.9-orbstack`), with a Go client binding a local port and
a server in the container logging the peer:

- **Published `127.0.0.1` port (bridge network)**: client bound `127.0.0.1:900`, server saw `192.168.215.1:35432`.
  Rewritten.
- **`network_mode: host`**: client `127.0.0.1:905`, server saw `127.0.0.1:33294`. Rewritten too.
- **The container's own IP from macOS**: `connect: no route to host` on both the default bridge and a compose network,
  for every binary tried (`ping`, `nc`, `curl`, the Go client), while the LAN gateway answers. Unreachable here.
  OrbStack advertises container-IP access, so it may work on another Mac, but it's a forwarder too, so expect the port
  rewritten.
- **Linux host → container IP over the bridge**, simulated from the OrbStack VM's root namespace (a `--network host`
  container dialing the bridge IP): client `192.168.215.1:900`, server saw `192.168.215.1:900`. Kept. From the same
  place, the published `127.0.0.1` port went through `docker-proxy` and was rewritten (`:901` → `:46264`).

And what knfsd answers, through the scratch probe that drove these (portmap DUMP, MOUNT EXPORT, MNT, then NFS NULL,
FSINFO, READDIRPLUS):

- **Through the published ports from macOS**: `/insecure` mounts and answers FSINFO and READDIRPLUS. `/secure` answers
  MNT with `MNT3ERR_ACCES` (13) even when the client bound port 900.
- **Direct, all from port 900**: `/secure` mounts and answers FSINFO and READDIRPLUS.
- **Direct, MNT from 901 then NFS from an ephemeral port**: the MNT succeeds, NFS `NULL` succeeds, and FSINFO and
  READDIRPLUS answer `NFS3ERR_PERM` (1). ❗ knfsd's per-call `secure` refusal is `NFS3ERR_PERM`, not `AUTH_TOOWEAK`.
  This is exactly the client that drops its reserved port after MOUNT, and the real server refuses it.
- **Direct, all ephemeral**: `/secure` answers MNT with `MNT3ERR_ACCES`.

So:

- **macOS**: no path keeps the source port, so a `/secure` cell can only assert the refusal through the published ports.
  The accepting half lives in `fake_nfs.rs` (its per-call port check) and on the Linux lane.
- **Linux CI**: the lane exports `NFS_FIXTURE_DIRECT_HOST`, and CI lowers `net.ipv4.ip_unprivileged_port_start` to 600
  so the unprivileged test process may bind 600–1023. A `/secure` cell dials that IP and skips, naming this section,
  when the variable is unset.

## Where the ports bind

Every mapping carries the `${NFS_BIND_ADDR:-127.0.0.1}` prefix. `AUTH_SYS` has no secret at all (any client may claim
any uid), so a writable export on `0.0.0.0` is a writable disk for the LAN and the tailnet of whoever runs the suite,
and the lease model makes these containers outlive the run that started them. `TestNfsFixturePortsBindToLoopback` fails
the run if a mapping loses the prefix. Set `NFS_BIND_ADDR=0.0.0.0` to reach the fixture from a NAT'd VM or a second
machine.

## The image is rebuilt on every bring-up

`image/` is first-party, so the stack declares it as a build context: its contents fold into the lease's config hash,
and `up` passes `--build`, so an edited entrypoint or seed reaches a stack that's already running.
