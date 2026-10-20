package checks

import (
	"net"
	"os"
	"os/exec"
	"runtime"
	"strconv"
	"strings"
	"time"
)

// The NFS fixture stack runs on a host-port range of its own, 15480+, disjoint
// from S3's 14480+, WebDAV's 13480+, SFTP's 12480+, cmdr's vendored
// `smb-consumer` stack at 11480+, and smb2's own harness at 10480+.
//
// Unlike every other stack, its one service publishes THREE ports, one per RPC
// program, so the table is keyed by program rather than by service:
//
//   - PORTMAP is rpcbind, host 15480 → container 111.
//   - NFS and MOUNT are `rpc.nfsd` and `rpc.mountd`, published on the SAME number
//     on both sides (`15481:15481`), because portmap answers the port a program
//     registered inside the container and that answer has to work from the host.
//
// The compose file reads each as `${NFS_FIXTURE_<PROGRAM>_PORT:-<default>}`, and
// the defaults match this table (`TestNfsFixturePortsMatchComposeDefaults`), so a
// bare `start.sh` lands on the same ports. Every child of this process (compose
// via the lease helper, cargo nextest) inherits the pinned values; the Rust cells
// read them back.
var nfsProgramHostPorts = map[string]int{
	"PORTMAP": 15480, "NFS": 15481, "MOUNT": 15482,
}

// nfsCoreServices are the services `core` mode brings up, which is the set the
// integration lane waits on. `TestNfsModeServicesAgree` keeps it equal to
// `stacklease.NFS`'s.
var nfsCoreServices = []string{"nfs-fixture-knfsd"}

// nfsBindAddrEnv names the interface every fixture port publishes on. The
// compose file defaults it to 127.0.0.1; nothing here sets it.
// `TestNfsFixturePortsBindToLoopback` fails the run if a `ports:` entry loses
// the prefix.
const nfsBindAddrEnv = "NFS_BIND_ADDR"

// NfsDirectHostEnv carries the knfsd container's own IP to the Rust cells, set
// only where dialing it keeps the client's source port: a Linux host, where the
// bridge routes host → container with no proxy in between. Every published port
// rewrites the source port (docker-proxy on Linux, OrbStack's forwarder on
// macOS), and `/secure` refuses any port past 1023, so this is the only way a
// cell reaches `/secure` through a reserved port. Unset means "no such path
// here", and the `/secure` cells skip saying so. The measurement behind it:
// `apps/desktop/test/nfs-servers/README.md` § "Reaching /secure".
const NfsDirectHostEnv = "NFS_FIXTURE_DIRECT_HOST"

// nfsKnfsdContainer is the compose container name of the one knfsd service.
const nfsKnfsdContainer = "nfs-fixture-nfs-fixture-knfsd-1"

// ApplyNfsPortEnv pins the NFS stack to its dedicated host-port range in the
// current process environment. Call once before bringing the stack up.
// Idempotent.
func ApplyNfsPortEnv() {
	for program, port := range nfsProgramHostPorts {
		_ = os.Setenv("NFS_FIXTURE_"+program+"_PORT", strconv.Itoa(port))
	}
}

// NfsFixtureServices lists every service the NFS core mode brings up, for the
// integration lane's readiness guard.
func NfsFixtureServices() []string { return append([]string(nil), nfsCoreServices...) }

// ApplyNfsDirectHostEnv exports `NFS_FIXTURE_DIRECT_HOST` when the knfsd
// container's own IP is reachable from here with the source port intact, and
// clears it otherwise. Only Linux qualifies (see `NfsDirectHostEnv`); a reachable
// IP on macOS still goes through OrbStack's forwarder. The TCP probe keeps a
// Linux box whose Docker runs in a VM (Docker Desktop) from advertising an
// address nothing can reach.
func ApplyNfsDirectHostEnv() {
	_ = os.Unsetenv(NfsDirectHostEnv)
	if runtime.GOOS != "linux" {
		return
	}
	out, err := RunCommand(exec.Command("docker", "inspect", "-f",
		"{{range .NetworkSettings.Networks}}{{.IPAddress}} {{end}}", nfsKnfsdContainer), true)
	if err != nil {
		return
	}
	for _, ip := range strings.Fields(out) {
		conn, dialErr := net.DialTimeout("tcp", net.JoinHostPort(ip, "111"), 2*time.Second)
		if dialErr != nil {
			continue
		}
		_ = conn.Close()
		_ = os.Setenv(NfsDirectHostEnv, ip)
		return
	}
}
