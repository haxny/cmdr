package main

import (
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sync"
)

const (
	noticesFile          = "THIRD-PARTY-NOTICES.md"
	noticesPackagesFile  = "apps/desktop/src/lib/licensing/third-party-packages.gen.json"
	noticesCommitSubject = "chore(deps): regenerate third-party notices"

	// checkRunnerEnv points the hooks at a stand-in for `scripts/check`. Only the
	// tests set it: the throwaway repos they push from have no check runner.
	checkRunnerEnv = "CMDR_GIT_HOOKS_CHECK_RUNNER"
)

// noticesInputs are the files whose change in a push can make the committed
// notices stale: the two lockfiles plus the license policy and the hand-kept
// credits, mirroring the `desktop-third-party-notices` check's own inputs.
var noticesInputs = map[string]bool{
	"Cargo.lock":     true,
	"pnpm-lock.yaml": true,
	"deny.toml":      true,
	"scripts/check/checks/third-party-vendored.json": true,
}

// checkRunner runs the repo's check runner with args and returns its combined output.
type checkRunner func(args ...string) (string, error)

// errRunnerUnavailable means the check runner couldn't start because it doesn't
// build. That's the hooks' "can't run" case, never a reason to stop a push.
var errRunnerUnavailable = errors.New("the check runner doesn't build")

// newCheckRunner returns the worktree's check runner, or nil when it can't have one:
// a checkout from before `scripts/check` existed, or one without `node_modules`,
// which most lanes need. Either makes the checks below skip.
func newCheckRunner(r repo) checkRunner {
	if fake := os.Getenv(checkRunnerEnv); fake != "" {
		return commandRunner(r.root, fake)
	}
	dir := filepath.Join(r.root, "scripts", "check")
	if !isDir(dir) || !isDir(filepath.Join(r.root, "node_modules")) {
		return nil
	}
	return builtRunner(r, dir)
}

// builtRunner builds the check runner once, into the worktree's git directory the
// way `.githooks/run-hook` builds this program, then runs the binary. Building
// first is what tells "the runner doesn't build" apart from "the check failed" in
// the skip message, which `go run` reports with the same exit code.
func builtRunner(r repo, dir string) checkRunner {
	var once sync.Once
	var bin, buildOut string
	var buildErr error
	return func(args ...string) (string, error) {
		once.Do(func() {
			if bin, buildErr = r.gitPath("cmdr-check-runner"); buildErr != nil {
				return
			}
			cmd := exec.Command("go", "build", "-buildvcs=false", "-o", bin, ".")
			cmd.Dir = dir
			out, err := cmd.CombinedOutput()
			buildOut, buildErr = string(out), err
		})
		if buildErr != nil {
			return buildOut, fmt.Errorf("%w: %v", errRunnerUnavailable, buildErr)
		}
		return commandRunner(dir, bin)(args...)
	}
}

func commandRunner(dir, name string, prefix ...string) checkRunner {
	return func(args ...string) (string, error) {
		cmd := exec.Command(name, append(append([]string{}, prefix...), args...)...)
		cmd.Dir = dir
		out, err := cmd.CombinedOutput()
		return string(out), err
	}
}

// regenerateNotices keeps a dependency push from reaching CI with stale license
// notices, which turned `main` red 26 times in three months
// (`docs/notes/ci-health-2026-10.md`). It only runs when the push moves one of
// `noticesInputs`, because the regeneration is a fresh `cargo-about` walk of the
// whole dependency graph (about a minute). Like the formatter step, it commits what
// it rewrote and stops the push.
func regenerateNotices(r repo, check checkRunner, pushed pushedRef, stderr io.Writer) (stopPush bool, err error) {
	if check == nil || !touchesNoticesInputs(r, pushed) {
		return false, nil
	}
	fixable, err := fixableFiles(r, []string{noticesFile, noticesPackagesFile})
	if err != nil || len(fixable) == 0 {
		return false, err
	}
	fmt.Fprintln(stderr, "This push moves a lockfile, so the hook is regenerating THIRD-PARTY-NOTICES.md (about a minute)...")
	// `--fresh`: a cache hit would skip the very rewrite this is for. `--allow-main`:
	// the main clone is where pushes happen.
	if out, err := check("--check", "desktop-third-party-notices", "--exact", "--fresh", "--allow-main", "--no-log"); err != nil {
		fmt.Fprintf(stderr, "notices hook: skipped, the check runner didn't finish:\n%s", out)
		return false, nil
	}
	modified, err := r.paths("diff", "--name-only", "-z")
	if err != nil {
		return false, err
	}
	regenerated := intersect(fixable, toSet(modified))
	if len(regenerated) == 0 {
		return false, nil
	}
	sha, err := commitOnly(r, regenerated, noticesCommitSubject)
	if err != nil {
		return false, err
	}
	fmt.Fprintf(stderr, "Regenerated the third-party notices and committed them as %s (%q).\n"+
		"Git can't add a commit to a push that's already running, so this push stopped. %s\n",
		sha, noticesCommitSubject, pushAgain(r, pushed))
	return true, nil
}

// touchesNoticesInputs reports whether the push changes any of `noticesInputs`. The
// range runs from the remote tip to the pushed commit. A new branch has no remote
// tip, and this clone may not have the one it's told about, so then the range is
// what the pushed commit adds over every remote-tracking ref. When even that can't
// be read, it counts as a change: regenerating once too often only costs a minute.
func touchesNoticesInputs(r repo, pushed pushedRef) bool {
	var changed []string
	var err error
	if !isZeroSha(pushed.remoteSha) && r.hasCommit(pushed.remoteSha) {
		changed, err = r.paths("diff", "--name-only", "-z", pushed.remoteSha, pushed.localSha)
	} else {
		changed, err = r.paths("log", "--format=", "--name-only", "-z", pushed.localSha, "--not", "--remotes")
	}
	if err != nil {
		return true
	}
	for _, p := range changed {
		if noticesInputs[p] {
			return true
		}
	}
	return false
}
