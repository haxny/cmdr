package checks

import (
	"regexp"
	"strconv"
	"strings"
	"testing"

	"cmdr/scripts/check/stacklease"
)

// The NFS stack makes the same promises as S3's (`s3_fixture_paths_test.go`):
// Go decides the host ports and the compose file carries matching `:-` defaults,
// every port binds to loopback, and the mode table agrees across the lease
// registry, `start.sh`, and the lane's readiness list. Plus one of its own: the
// portmap port trick, where `rpc.nfsd` and `rpc.mountd` must listen on the very
// number the host publishes.

// nfsComposePortsBlockRE captures the one service's block-style `ports:` list.
var nfsComposePortsBlockRE = regexp.MustCompile(`ports:\n((?:[ \t]+- '[^']*'\n)+)`)

// nfsComposePortItemRE captures one item of that list.
var nfsComposePortItemRE = regexp.MustCompile(`- '([^']*)'`)

func nfsComposePortItems(t *testing.T, compose string) []string {
	t.Helper()
	blocks := nfsComposePortsBlockRE.FindAllStringSubmatch(compose, -1)
	if len(blocks) != 1 {
		t.Fatalf("%s has %d block-style `ports:` lists; the stack has one service, so this guard is reading the wrong shape", nfsComposeRel, len(blocks))
	}
	var items []string
	for _, m := range nfsComposePortItemRE.FindAllStringSubmatch(blocks[0][1], -1) {
		items = append(items, m[1])
	}
	return items
}

func TestNfsFixturePortsMatchComposeDefaults(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, nfsComposeRel)

	// Every `${NFS_FIXTURE_X_PORT:-N}` occurrence, not just the first: the NFS
	// and MOUNT ports each appear on both sides of a mapping and in the env the
	// entrypoint reads, and one stale copy splits the server from its portmap
	// answer.
	composePorts := map[string]int{}
	for _, m := range composeDefaultRE.FindAllStringSubmatch(compose, -1) {
		program, ok := strings.CutPrefix(m[1], "NFS_FIXTURE_")
		if !ok {
			continue
		}
		program, ok = strings.CutSuffix(program, "_PORT")
		if !ok || program == "" {
			continue
		}
		port, err := strconv.Atoi(m[2])
		if err != nil {
			t.Errorf("%s: ${%s:-%s} has a non-numeric default", nfsComposeRel, m[1], m[2])
			continue
		}
		if seen, dup := composePorts[program]; dup && seen != port {
			t.Errorf("%s defaults NFS_FIXTURE_%s_PORT to both %d and %d; every copy has to agree, or the daemon listens on one port while the host publishes another", nfsComposeRel, program, seen, port)
		}
		composePorts[program] = port
	}

	for program, want := range nfsProgramHostPorts {
		got, ok := composePorts[program]
		if !ok {
			t.Errorf("nfsProgramHostPorts has %s (%d) but %s declares no ${NFS_FIXTURE_%s_PORT:-…} default; a bare start.sh would land it somewhere the suite never looks", program, want, nfsComposeRel, program)
			continue
		}
		if got != want {
			t.Errorf("%s defaults NFS_FIXTURE_%s_PORT to %d; nfsProgramHostPorts pins %d. A bare start.sh and a check run would then serve the same fixture on two ports", nfsComposeRel, program, got, want)
		}
	}

	extras := map[string]bool{}
	for program := range composePorts {
		if _, inTable := nfsProgramHostPorts[program]; !inTable {
			extras[program] = true
		}
	}
	if len(extras) != 0 {
		t.Errorf("%s declares port defaults for %s that nfsProgramHostPorts doesn't carry", nfsComposeRel, sortedKeys(extras))
	}
}

func TestNfsFixturePortsBindToLoopback(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, nfsComposeRel)

	const wantPrefix = "${" + nfsBindAddrEnv + ":-127.0.0.1}:"

	items := nfsComposePortItems(t, compose)
	if len(items) != len(nfsProgramHostPorts) {
		t.Fatalf("%s publishes %d ports; nfsProgramHostPorts has %d programs. A program publishing nothing, or a second publish of one, means this guard is reading the wrong set", nfsComposeRel, len(items), len(nfsProgramHostPorts))
	}
	for _, item := range items {
		if !strings.HasPrefix(item, wantPrefix) {
			t.Errorf("%s publishes `%s` with no %q prefix, so Docker binds it on 0.0.0.0 and puts a writable AUTH_SYS export (no password, any uid) on the LAN and the tailnet of whoever runs the suite", nfsComposeRel, item, wantPrefix)
		}
	}
}

// The portmap port trick, pinned. portmap answers the port a program registered
// INSIDE the container, so a host client can use that answer only when the
// published host port and the container port are the same number. Only rpcbind
// itself may move (host PORTMAP → container 111).
func TestNfsFixturePublishesEachProgramOnItsOwnNumber(t *testing.T) {
	root := repoRootForTest(t)
	compose := readRepoFile(t, root, nfsComposeRel)

	const prefix = "${" + nfsBindAddrEnv + ":-127.0.0.1}:"
	published := map[string]string{}
	for _, item := range nfsComposePortItems(t, compose) {
		mapping := strings.TrimPrefix(item, prefix)
		m := composeDefaultRE.FindStringSubmatch(mapping)
		if m == nil {
			t.Errorf("%s publishes `%s`, whose host side names no NFS_FIXTURE_*_PORT variable", nfsComposeRel, item)
			continue
		}
		hostSide := m[0]
		containerSide, ok := strings.CutPrefix(mapping, hostSide+":")
		if !ok {
			t.Errorf("%s publishes `%s`, which isn't `<host>:<container>`", nfsComposeRel, item)
			continue
		}
		published[m[1]] = containerSide
		if m[1] == "NFS_FIXTURE_PORTMAP_PORT" {
			if containerSide != "111" {
				t.Errorf("%s publishes portmap onto container port %s; rpcbind listens on 111", nfsComposeRel, containerSide)
			}
			continue
		}
		if containerSide != hostSide {
			t.Errorf("%s publishes %s onto container port %s; it has to be the same `%s`, or portmap hands the host a port only the container can use", nfsComposeRel, hostSide, containerSide, hostSide)
		}
	}
	for program := range nfsProgramHostPorts {
		if _, ok := published["NFS_FIXTURE_"+program+"_PORT"]; !ok {
			t.Errorf("%s never publishes NFS_FIXTURE_%s_PORT", nfsComposeRel, program)
		}
	}

	// And the daemons are told those same numbers.
	for env, program := range map[string]string{"NFS_PORT": "NFS", "MOUNT_PORT": "MOUNT"} {
		want := env + ": '${NFS_FIXTURE_" + program + "_PORT:-"
		if !strings.Contains(compose, want) {
			t.Errorf("%s doesn't hand the entrypoint `%s` from NFS_FIXTURE_%s_PORT, so the daemon may listen on a port the host doesn't publish", nfsComposeRel, env, program)
		}
	}
}

func TestNfsModeServicesAgree(t *testing.T) {
	root := repoRootForTest(t)
	start := readRepoFile(t, root, nfsStartRel)

	// `start.sh` shares WebDAV's case-table shape, so the same two patterns read it.
	fromStart := map[string][]string{}
	for _, arm := range webdavStartModeRE.FindAllStringSubmatch(start, -1) {
		mode, body := arm[1], arm[2]
		if mode == "all" {
			fromStart[mode] = nil
			continue
		}
		m := webdavStartServicesRE.FindStringSubmatch(body)
		if m == nil {
			t.Errorf("%s: the %q arm assigns no `services=(…)`, so it would bring up the WHOLE project", nfsStartRel, mode)
			continue
		}
		fromStart[mode] = strings.Fields(m[1])
	}

	for _, mode := range stacklease.NFS.Modes() {
		want := stacklease.NFS.ServicesForMode(mode)
		got, ok := fromStart[mode]
		if !ok {
			t.Errorf("%s has no %q arm; the lease registry serves that mode, so a bare `start.sh %s` would refuse a mode a check run accepts", nfsStartRel, mode, mode)
			continue
		}
		if strings.Join(got, " ") != strings.Join(want, " ") {
			t.Errorf("%s brings up %v for %q; the lease registry brings up %v. A cell then talks to a server nobody started", nfsStartRel, got, mode, want)
		}
	}
	for mode := range fromStart {
		if !contains(stacklease.NFS.Modes(), mode) {
			t.Errorf("%s offers the mode %q the lease registry doesn't serve, so `start.sh %s` and a check run disagree about what is up", nfsStartRel, mode, mode)
		}
	}

	if got, want := strings.Join(NfsFixtureServices(), " "), strings.Join(stacklease.NFS.ServicesForMode(stacklease.ModeCore), " "); got != want {
		t.Errorf("the integration lane waits on %v; NFS's core mode brings up %v. Waiting on a container the mode never starts burns the whole timeout", got, want)
	}
	if want := stacklease.NFS.ProjectName + "-" + NfsFixtureServices()[0] + "-1"; nfsKnfsdContainer != want {
		t.Errorf("nfsKnfsdContainer %q isn't the compose container of %q, so the direct-host lookup would inspect nothing", nfsKnfsdContainer, NfsFixtureServices()[0])
	}
}
