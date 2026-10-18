package main

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// fakeCheckRunner stands in for `scripts/check`: it logs every invocation, rewrites
// the notices when asked for them, and fails the fast lane while the file named by
// FAKE_BUDGET_FAIL exists (so a hook that ran it would stop the push).
const fakeCheckRunner = `#!/bin/sh
echo "$*" >> "$FAKE_RUNNER_LOG"
case "$*" in
  *desktop-third-party-notices*) printf 'regenerated\n' > THIRD-PARTY-NOTICES.md ;;
  *--fast*) if [ -f "$FAKE_BUDGET_FAIL" ]; then echo "file-length FAILED: a.go is over budget"; exit 1; fi ;;
esac
exit 0
`

type fakeRunner struct {
	log, budgetFail string
}

// useFakeCheckRunner points the hook at fakeCheckRunner for this test.
func useFakeCheckRunner(t *testing.T) fakeRunner {
	t.Helper()
	dir := t.TempDir()
	script := filepath.Join(dir, "check")
	if err := os.WriteFile(script, []byte(fakeCheckRunner), 0o755); err != nil {
		t.Fatal(err)
	}
	f := fakeRunner{log: filepath.Join(dir, "log"), budgetFail: filepath.Join(dir, "budget-fail")}
	t.Setenv(checkRunnerEnv, script)
	t.Setenv("FAKE_RUNNER_LOG", f.log)
	t.Setenv("FAKE_BUDGET_FAIL", f.budgetFail)
	return f
}

func (f fakeRunner) calls(t *testing.T) string {
	t.Helper()
	content, err := os.ReadFile(f.log)
	if os.IsNotExist(err) {
		return ""
	}
	if err != nil {
		t.Fatal(err)
	}
	return string(content)
}

// pushedBase commits the notices and a lockfile, pushes them without the hook, and
// checks out `feature` from there, so each test starts from a remote that already
// has both.
func pushedBase(r *testRepo) {
	r.write(noticesFile, "old\n")
	r.write("Cargo.lock", "v1\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "base")
	r.git("push", "-q", "--no-verify", "origin", "main", "main:feature")
	r.git("checkout", "-q", "-b", "feature", "--track", "origin/feature")
}

func TestPrePushRegeneratesTheNoticesWhenALockfileMoved(t *testing.T) {
	f := useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.write("Cargo.lock", "v2\n")
	r.git("commit", "-q", "-am", "bump a crate")
	before := r.commitCount()

	out, err := r.tryGit("push", "origin", "feature")
	if err == nil {
		t.Fatalf("the push went through with stale notices:\n%s", out)
	}
	if !strings.Contains(out, "Push again") {
		t.Errorf("the stopped push doesn't say what to do next:\n%s", out)
	}
	if got := r.commitCount(); got != before+1 {
		t.Fatalf("commits after the stopped push: got %d, want %d", got, before+1)
	}
	assertEqual(t, "notices commit subject", strings.TrimSpace(r.git("log", "-1", "--format=%s")), noticesCommitSubject)
	assertEqual(t, "committed notices", r.committed("HEAD", noticesFile), "regenerated\n")
	assertEqual(t, "status", r.status(), "")

	r.git("push", "-q", "origin", "feature")
	assertEqual(t, "pushed notices", r.committed("remote:feature", noticesFile), "regenerated\n")
	if !strings.Contains(f.calls(t), "--check desktop-third-party-notices") {
		t.Errorf("the hook never asked the runner for the notices:\n%s", f.calls(t))
	}
}

func TestPrePushLeavesTheNoticesAloneWithoutALockfileChange(t *testing.T) {
	f := useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.write("other.txt", "x\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "unrelated")

	r.git("push", "-q", "origin", "feature")

	if strings.Contains(f.calls(t), "third-party-notices") {
		t.Errorf("the hook regenerated the notices for a push that moved no lockfile:\n%s", f.calls(t))
	}
	assertEqual(t, "pushed notices", r.committed("remote:feature", noticesFile), "old\n")
}

// `land` pushes a lockfile change to `main` only after CI passed it, notices included,
// and regenerating them there would be a minute of `cargo-about` for nothing.
func TestPrePushLeavesTheNoticesAloneOnAPushToMain(t *testing.T) {
	f := useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.write("Cargo.lock", "v2\n")
	r.git("commit", "-q", "-am", "bump a crate")

	r.git("push", "-q", "origin", "HEAD:main")

	if f.calls(t) != "" {
		t.Errorf("the hook ran the check runner for a push to main:\n%s", f.calls(t))
	}
}

// A pushed branch gets every lane from CI on its PR, which is why agents push between
// milestones, so the hook leaves the check lanes to CI.
func TestPrePushLeavesTheFastLaneToCI(t *testing.T) {
	f := useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.write("other.txt", "x\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "grow a file")
	if err := os.WriteFile(f.budgetFail, nil, 0o644); err != nil {
		t.Fatal(err)
	}

	r.git("push", "-q", "origin", "feature")

	if strings.Contains(f.calls(t), "--fast") {
		t.Errorf("the hook ran the fast lane for a branch push:\n%s", f.calls(t))
	}
}

// A commit the hook adds lands on HEAD, which `push-staging` doesn't follow, so
// "push again" alone would resend the stale commit. The message says what to push.
func TestPrePushRegeneratesTheNoticesForAnotherNameForHEAD(t *testing.T) {
	useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.write("Cargo.lock", "v2\n")
	r.git("commit", "-q", "-am", "bump a crate")
	r.git("branch", "push-staging")

	out, err := r.tryGit("push", "origin", "push-staging:feature")
	if err == nil {
		t.Fatalf("the push went through with stale notices:\n%s", out)
	}
	assertEqual(t, "committed notices", r.committed("HEAD", noticesFile), "regenerated\n")
	if !strings.Contains(out, "git push origin HEAD:feature") {
		t.Errorf("the stopped push doesn't say to push HEAD:\n%s", out)
	}

	r.git("push", "-q", "origin", "HEAD:feature")
	assertEqual(t, "pushed notices", r.committed("remote:feature", noticesFile), "regenerated\n")
}

// The working tree doesn't describe another branch's commit, so the checks skip it,
// and say so: a silent skip is how stale notices reached `main` before.
func TestPrePushSaysItSkippedAPushOfACommitThatIsNotCheckedOut(t *testing.T) {
	f := useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.write("Cargo.lock", "v2\n")
	r.git("commit", "-q", "-am", "bump a crate")
	r.git("branch", "other", "HEAD~1")

	out := r.git("push", "origin", "other")

	if f.calls(t) != "" {
		t.Errorf("the hook ran checks for a commit that isn't checked out:\n%s", f.calls(t))
	}
	if !strings.Contains(out, "isn't checked out") {
		t.Errorf("the push doesn't say the checks were skipped:\n%s", out)
	}
}

// A new remote branch has no remote tip to diff against, so the range is what the
// push adds over every remote-tracking ref.
func TestPrePushReadsANewBranchRangeAgainstTheRemote(t *testing.T) {
	f := useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.git("checkout", "-q", "-b", "feature-2")
	r.write("other.txt", "x\n")
	r.git("add", ".")
	r.git("commit", "-q", "-m", "unrelated")

	r.git("push", "-q", "origin", "feature-2")
	if strings.Contains(f.calls(t), "third-party-notices") {
		t.Errorf("the hook regenerated the notices for a new branch that moved no lockfile:\n%s", f.calls(t))
	}

	r.write("Cargo.lock", "v2\n")
	r.git("commit", "-q", "-am", "bump a crate")
	if out, err := r.tryGit("push", "origin", "HEAD:refs/heads/feature-3"); err == nil {
		t.Fatalf("a new branch with a lockfile change went through with stale notices:\n%s", out)
	}
}

func TestPrePushLeavesADeletionAlone(t *testing.T) {
	f := useFakeCheckRunner(t)
	r := newTestRepo(t)
	pushedBase(r)
	r.git("push", "-q", "--no-verify", "origin", "HEAD:refs/heads/gone")

	out := r.git("push", "origin", ":gone")

	if f.calls(t) != "" {
		t.Errorf("the hook ran checks for a deletion:\n%s", f.calls(t))
	}
	if strings.Contains(out, "skipped") {
		t.Errorf("a deletion printed a skip notice:\n%s", out)
	}
}

// A check runner that doesn't build can't answer, and the hooks never stand in the
// way when they can't run: the push goes on, saying the step was skipped.
func TestPrePushLetsThePushThroughWhenTheRunnerDoesNotBuild(t *testing.T) {
	r := newTestRepo(t)
	pushedBase(r)
	r.write("scripts/check/go.mod", "module broken\n\ngo 1.22\n")
	r.write("scripts/check/main.go", "package main\n\nfunc main() { not go }\n")
	if err := os.MkdirAll(filepath.Join(r.dir, "node_modules"), 0o755); err != nil {
		t.Fatal(err)
	}
	r.write("Cargo.lock", "v2\n")
	r.git("commit", "-q", "-am", "bump a crate")
	hookRepo := repo{root: r.dir}
	pushed := pushedRef{localRef: "refs/heads/feature", localSha: strings.TrimSpace(r.git("rev-parse", "HEAD")),
		remoteRef: "refs/heads/feature", remoteSha: strings.TrimSpace(r.git("rev-parse", "origin/feature"))}
	var stderr strings.Builder
	stop, err := regenerateNotices(hookRepo, newCheckRunner(hookRepo), pushed, &stderr)
	if err != nil || stop {
		t.Fatalf("a runner that doesn't build stopped the push (stop=%v, err=%v):\n%s", stop, err, stderr.String())
	}
	if !strings.Contains(stderr.String(), "skipped") {
		t.Errorf("the hook didn't say it skipped the notices:\n%s", stderr.String())
	}
}
