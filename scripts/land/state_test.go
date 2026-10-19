package main

import (
	"testing"
	"time"
)

func TestNextStartStep(t *testing.T) {
	ready := branchState{branch: "worktree-x", based: true}
	cases := []struct {
		name  string
		state branchState
		want  startStep
	}{
		{"ready to go", ready, stepProceed},
		{"main itself", branchState{branch: "main", based: true}, stepRefuseBranch},
		{"detached HEAD", branchState{branch: "", based: true}, stepRefuseBranch},
		{"dirty tree", branchState{branch: "worktree-x", dirty: true, based: true}, stepRefuseDirty},
		{"dirty beats landed", branchState{branch: "worktree-x", dirty: true, landed: true, based: true}, stepRefuseDirty},
		{"already landed", branchState{branch: "worktree-x", landed: true, based: true}, stepFinish},
		{"main moved", branchState{branch: "worktree-x"}, stepRebase},
		{"main moved under a merge commit", branchState{branch: "worktree-x", hasMerges: true}, stepRefuseMergeRebase},
		{"a merge commit on a current branch", branchState{branch: "worktree-x", based: true, hasMerges: true}, stepProceed},
	}
	for _, c := range cases {
		if got := nextStartStep(c.state); got != c.want {
			t.Errorf("%s: got %d, want %d", c.name, got, c.want)
		}
	}
}

var (
	first  = commit{Subject: "feat: search finds files by size", Body: "- `SizeFilter` parses `>10mb`"}
	second = commit{Subject: "fix: the size filter rounds like Finder"}
	third  = commit{Subject: "docs: the search guide covers size", Body: "Two lines\nof body"}
)

func TestPRTitleIsTheOldestSubject(t *testing.T) {
	if got := prTitle([]commit{first, second}); got != first.Subject {
		t.Errorf("got %q", got)
	}
}

func TestPRBodyListsEveryCommit(t *testing.T) {
	want := "### feat: search finds files by size\n\n- `SizeFilter` parses `>10mb`\n\n" +
		"### fix: the size filter rounds like Finder\n\n" +
		"### docs: the search guide covers size\n\nTwo lines\nof body\n"
	if got := prBody([]commit{first, second, third}); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
}

func TestSquashMessage(t *testing.T) {
	want := "feat: search finds files by size\n\n- `SizeFilter` parses `>10mb`\n\n" +
		"fix: the size filter rounds like Finder\n\n" +
		"docs: the search guide covers size\n\nTwo lines\nof body\n"
	if got := squashMessage([]commit{first, second, third}); got != want {
		t.Errorf("got:\n%s\nwant:\n%s", got, want)
	}
	if got, want := squashMessage([]commit{second}), "fix: the size filter rounds like Finder\n"; got != want {
		t.Errorf("one commit: got %q, want %q", got, want)
	}
}

func TestNeedsSquash(t *testing.T) {
	if needsSquash([]commit{first}, "") {
		t.Error("one commit and no message: nothing to squash, so a re-run is a no-op")
	}
	if !needsSquash([]commit{first, second}, "") {
		t.Error("two commits squash")
	}
	if !needsSquash([]commit{first}, "feat: a better title") {
		t.Error("a new message rewrites even one commit")
	}
	if needsSquash([]commit{first}, first.Subject+"\n\n"+first.Body) {
		t.Error("the same message again is a no-op")
	}
}

func TestCIVerdict(t *testing.T) {
	at := func(m int) time.Time { return time.Date(2026, 10, 18, 12, m, 0, 0, time.UTC) }
	running := workflowRun{ID: 2, Status: "in_progress", CreatedAt: at(5)}
	red := workflowRun{ID: 1, Status: "completed", Conclusion: "failure", CreatedAt: at(1)}
	done := workflowRun{ID: 2, Status: "completed", Conclusion: "success", CreatedAt: at(5)}
	ok := checkRun{ID: 20, Status: "completed", Conclusion: "success"}
	failed := checkRun{ID: 10, Status: "completed", Conclusion: "failure"}
	skipped := checkRun{ID: 30, Status: "completed", Conclusion: "skipped"}

	cases := []struct {
		name   string
		runs   []workflowRun
		checks []checkRun
		want   verdict
		wantID int64
	}{
		{"no run yet", nil, nil, verdictMissing, 0},
		{"running", []workflowRun{running}, nil, verdictPending, 2},
		{"a re-run in flight outranks the old red check", []workflowRun{red, running}, []checkRun{failed}, verdictPending, 2},
		{"green", []workflowRun{done}, []checkRun{failed, ok}, verdictGreen, 2},
		{"the newest check decides", []workflowRun{done}, []checkRun{ok, {ID: 40, Status: "completed", Conclusion: "failure"}}, verdictRed, 2},
		{"red run", []workflowRun{red}, []checkRun{failed}, verdictRed, 1},
		{"finished with no CI OK at all", []workflowRun{red}, nil, verdictRed, 1},
		{"a skipped CI OK is not green", []workflowRun{done}, []checkRun{skipped}, verdictRed, 2},
	}
	for _, c := range cases {
		got, run := ciVerdict(c.runs, c.checks)
		if got != c.want || run.ID != c.wantID {
			t.Errorf("%s: got %d (run %d), want %d (run %d)", c.name, got, run.ID, c.want, c.wantID)
		}
	}
}

func TestOpenPR(t *testing.T) {
	prs := []pullRequest{{Number: 1, State: "CLOSED"}, {Number: 2, State: "OPEN"}, {Number: 3, State: "MERGED"}}
	if pr, ok := openPR(prs); !ok || pr.Number != 2 {
		t.Errorf("got %+v %v", pr, ok)
	}
	if _, ok := openPR(prs[:1]); ok {
		t.Error("a closed PR isn't reused")
	}
}

func TestTeardownCommand(t *testing.T) {
	cases := []struct{ root, want string }{
		{"/repo/.claude/worktrees/cursor-fix", "~/.claude/scripts/remove-worktree.sh cursor-fix"},
		{"/repo/.claude/worktrees/cursor-fix/", "~/.claude/scripts/remove-worktree.sh cursor-fix"},
		{"/repo", ""},
		{"/elsewhere/cursor-fix", ""},
	}
	for _, c := range cases {
		if got := teardownCommand(c.root); got != c.want {
			t.Errorf("%s: got %q, want %q", c.root, got, c.want)
		}
	}
}

func TestWorktrees(t *testing.T) {
	porcelain := "worktree /repo\nHEAD abc\nbranch refs/heads/main\n\n" +
		"worktree /repo/.claude/worktrees/x\nHEAD def\nbranch refs/heads/worktree-x\n\n" +
		"worktree /repo/.claude/worktrees/y\nHEAD 123\ndetached\n"
	wts := parseWorktrees(porcelain)
	if len(wts) != 3 {
		t.Fatalf("got %+v", wts)
	}
	if wt, ok := worktreeOf(wts, "main"); !ok || wt.path != "/repo" {
		t.Errorf("main: got %+v %v", wt, ok)
	}
	if wt, ok := worktreeOf(wts, "worktree-x"); !ok || wt.path != "/repo/.claude/worktrees/x" {
		t.Errorf("worktree-x: got %+v %v", wt, ok)
	}
	if _, ok := worktreeOf(wts, "nope"); ok {
		t.Error("nope: found")
	}
}
