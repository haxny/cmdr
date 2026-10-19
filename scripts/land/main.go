// Land: takes a worktree branch onto `main` through CI, the only way commits reach `main` once
// its rulesets are on (`.github/rulesets/`).
//
// In order, it:
//
//  1. refuses a dirty tree, fetches, and rebases onto `origin/main` when it moved (a branch
//     carrying a merge commit stops instead: a plain rebase would flatten it);
//  2. with `-squash`, squashes the branch into one commit (`-m` for the message, else every
//     commit message, oldest first);
//  3. pushes the branch and opens its PR, or reuses the open one (the PR is plumbing: CI runs on
//     `pull_request`, and nobody reviews it);
//  4. waits for `CI OK` on the head SHA, and on red names the failed jobs and the re-run command;
//  5. lands with `git push origin <sha>:main`: the admin bypasses the PR ruleset while the CI
//     ruleset still demands `CI OK` on that exact SHA, the SHAs stay the same, and GitHub marks
//     the PR merged;
//  6. fast-forwards local `main` (wherever it's checked out), deletes the remote branch, and
//     prints the command that removes the worktree.
//
// When `origin/main` moves while CI runs, it rebases and waits for CI again, up to
// `maxCIRuns` runs in all.
//
// Every run reads where the branch stands and picks up from there, so it's safe to re-run after a
// flake re-run, a rebase, or a closed laptop.
//
// # Usage
//
//	pnpm land                 push, wait for CI, land
//	pnpm land -no-wait        push and open the PR, then return (offload CI between milestones)
//	pnpm land -squash [-m M]  squash first
//	pnpm land rulesets [-apply]
//
// Docs: `scripts/land/CLAUDE.md`.
package main

import (
	"errors"
	"flag"
	"fmt"
	"net/url"
	"os"
	"time"
)

const (
	repo       = "vdavid/cmdr"
	ciWorkflow = "ci.yml"
	ciCheck    = "CI OK"
	// Every PR this script opens is an agent's, and agent-made issues and PRs carry this label.
	agentLabel = "claude-created"
	pollEvery  = 30 * time.Second
	// How long a CI run gets to show up after the push that starts it.
	appearWithin = 5 * time.Minute
	// How many CI runs one land waits through while `origin/main` keeps moving under it. High
	// enough that several sessions landing at once take turns rather than giving up; a `main`
	// busier than this needs a person, not a ninth run.
	maxCIRuns = 8
)

// errMainMoved: `origin/main` moved while CI ran, so the green SHA no longer fast-forwards it.
var errMainMoved = errors.New("origin/main moved while CI ran")

type config struct {
	root    string
	squash  bool
	message string
	noWait  bool
	timeout time.Duration
}

func main() {
	root := flag.String("root", "", "the worktree to land (scripts/land.sh sets it)")
	squash := flag.Bool("squash", false, "squash the branch into one commit before pushing")
	message := flag.String("m", "", "with -squash: the commit message (default: every commit message, oldest first)")
	noWait := flag.Bool("no-wait", false, "push and open the PR, then return without waiting for CI or landing")
	timeout := flag.Duration("timeout", 2*time.Hour, "how long to wait for CI")
	flag.Usage = func() {
		fmt.Fprintln(os.Stderr, "Usage: pnpm land [-no-wait] [-squash [-m MESSAGE]] [-timeout 2h]")
		fmt.Fprintln(os.Stderr, "       pnpm land rulesets [-apply]")
		flag.PrintDefaults()
	}
	flag.Parse()
	if *root == "" {
		flag.Usage()
		os.Exit(2)
	}
	r := runner{dir: *root}
	if flag.NArg() > 0 {
		if flag.Arg(0) != "rulesets" {
			flag.Usage()
			os.Exit(2)
		}
		if err := rulesets(r, flag.Args()[1:]); err != nil {
			fail(err)
		}
		return
	}
	if *message != "" && !*squash {
		fail(errors.New("-m only works with -squash"))
	}
	cfg := config{root: *root, squash: *squash, message: *message, noWait: *noWait, timeout: *timeout}
	if err := land(r, cfg); err != nil {
		fail(err)
	}
}

// land runs one land, and another on top of a rebase each time `origin/main` moves while CI runs,
// up to `maxCIRuns` CI runs in all.
func land(r runner, cfg config) error {
	for run := 1; ; run++ {
		err := landOnce(r, cfg)
		if !errors.Is(err, errMainMoved) {
			return err
		}
		if run >= maxCIRuns {
			return fmt.Errorf("origin/main moved during each of %d CI runs, so I stopped. Land again when it's quieter: pnpm land", maxCIRuns)
		}
		say("origin/main moved while CI ran, so rebasing and running CI again (run %d of %d).", run+1, maxCIRuns)
	}
}

func landOnce(r runner, cfg config) error {
	state, err := readState(r)
	if err != nil {
		return err
	}
	if proceed, err := start(r, state); !proceed {
		return err
	}
	commits, err := r.commits("origin/main..HEAD")
	if err != nil {
		return err
	}
	if cfg.squash && needsSquash(commits, cfg.message) {
		if err := squash(r, commits, cfg.message); err != nil {
			return err
		}
	}
	head, err := r.git("rev-parse", "HEAD")
	if err != nil {
		return err
	}
	if head, err = pushBranch(r, state.branch, head); err != nil {
		return err
	}
	if commits, err = r.commits("origin/main..HEAD"); err != nil {
		return err
	}
	pr, err := ensurePR(r, state.branch, commits)
	if err != nil {
		return err
	}
	if cfg.noWait {
		say("CI is on it: %s\nLand it when you're ready: pnpm land", pr.URL)
		return nil
	}
	if err := waitForCI(r, head, cfg.timeout); err != nil {
		return err
	}
	if err := landOnMain(r, head); err != nil {
		return err
	}
	say("Landed %s on main; GitHub marks %s merged.", head[:12], pr.URL)
	return finish(r, state.branch)
}

// start acts on the start step: it refuses, finishes an earlier land, or rebases. It reports
// whether the land goes on.
func start(r runner, state branchState) (bool, error) {
	switch nextStartStep(state) {
	case stepRefuseBranch:
		return false, errors.New("check out a feature branch first: land takes a branch onto main, never main itself")
	case stepRefuseDirty:
		return false, errors.New("commit your changes first: land pushes what's committed, and the tree has uncommitted changes")
	case stepFinish:
		say("%s is already on origin/main.", state.branch)
		return false, finish(r, state.branch)
	case stepRefuseMergeRebase:
		return false, errors.New("origin/main moved, and this branch carries a merge commit that a plain rebase would flatten. Catch up by hand, then land again:\n  git rebase --rebase-merges origin/main   (a contributor's PR record: docs/guides/handling-prs.md)\n  or redo the merge on top of main         (a release merge-back: docs/guides/releasing.md)")
	case stepRebase:
		say("Rebasing onto origin/main...")
		if _, err := r.git("rebase", "origin/main"); err != nil {
			_, _ = r.git("rebase", "--abort")
			return false, fmt.Errorf("the rebase hit conflicts, so I put the branch back. Rebase by hand, then land again:\n  git rebase origin/main\n\n%w", err)
		}
	case stepProceed:
	}
	return true, nil
}

func readState(r runner) (branchState, error) {
	branch, err := r.git("branch", "--show-current")
	if err != nil {
		return branchState{}, err
	}
	if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
		return branchState{}, err
	}
	// Untracked files can't change what lands, and the main clone (where `release.sh` lands from)
	// often has some.
	status, err := r.git("status", "--porcelain", "--untracked-files=no")
	if err != nil {
		return branchState{}, err
	}
	merges, err := r.git("rev-list", "--merges", "origin/main..HEAD")
	if err != nil {
		return branchState{}, err
	}
	return branchState{
		branch:    branch,
		dirty:     status != "",
		landed:    r.isAncestor("HEAD", "origin/main"),
		based:     r.isAncestor("origin/main", "HEAD"),
		hasMerges: merges != "",
	}, nil
}

func squash(r runner, commits []commit, message string) error {
	if message == "" {
		message = squashMessage(commits)
	}
	say("Squashing %d %s into one...", len(commits), plural(len(commits), "commit", "commits"))
	if _, err := r.git("reset", "--soft", "origin/main"); err != nil {
		return err
	}
	if err := r.gitStdin(message, "commit", "--quiet", "-F", "-"); err != nil {
		return fmt.Errorf("the squash commit didn't go through; `git reflog` has the branch as it was\n%w", err)
	}
	return nil
}

// pushBranch pushes HEAD to its feature branch unless origin already has it. The lease names the
// SHA it just read, so a push from another machine in between is refused instead of overwritten.
//
// The pre-push hook may commit a formatting fix and stop the push; then this pushes once more
// with that commit on top. It returns the SHA that's on origin.
func pushBranch(r runner, branch, head string) (string, error) {
	remote, err := r.remoteSHA(branch)
	if err != nil {
		return "", err
	}
	for attempt := 0; remote != head; attempt++ {
		say("Pushing %s...", branch)
		_, err = r.git("push", "--quiet", fmt.Sprintf("--force-with-lease=refs/heads/%s:%s", branch, remote), "origin", "HEAD:refs/heads/"+branch)
		if err == nil {
			return head, nil
		}
		newHead, headErr := r.git("rev-parse", "HEAD")
		if headErr != nil || newHead == head || attempt > 0 {
			return "", fmt.Errorf("the branch push didn't go through; fix what it says, then land again: pnpm land\n%w", err)
		}
		say("The pre-push hook added a commit, so pushing again with it.")
		head = newHead
	}
	return head, nil
}

func ensurePR(r runner, branch string, commits []commit) (pullRequest, error) {
	var prs []pullRequest
	if err := r.ghJSON(&prs, "pr", "list", "-R", repo, "--head", branch, "--state", "all", "--json", "number,state,url"); err != nil {
		return pullRequest{}, err
	}
	if pr, ok := openPR(prs); ok {
		return pr, nil
	}
	url, err := r.gh("pr", "create", "-R", repo, "--base", "main", "--head", branch, "--title", prTitle(commits), "--body", prBody(commits), "--label", agentLabel)
	if err != nil {
		return pullRequest{}, err
	}
	say("Opened %s", url)
	return pullRequest{State: "OPEN", URL: url}, nil
}

func waitForCI(r runner, head string, timeout time.Duration) error {
	start := time.Now()
	announced := false
	for {
		var runs []workflowRun
		if err := r.ghJSON(&runs, "run", "list", "-R", repo, "--workflow", ciWorkflow, "--commit", head, "--json", "databaseId,status,conclusion,createdAt"); err != nil {
			return err
		}
		var checks struct {
			CheckRuns []checkRun `json:"check_runs"`
		}
		if err := r.ghJSON(&checks, "api", fmt.Sprintf("repos/%s/commits/%s/check-runs?check_name=%s&filter=all", repo, head, url.PathEscape(ciCheck))); err != nil {
			return err
		}
		v, run := ciVerdict(runs, checks.CheckRuns)
		switch v {
		case verdictGreen:
			say("%s is green.", ciCheck)
			return nil
		case verdictRed:
			return ciFailure(r, run)
		case verdictMissing:
			if time.Since(start) > appearWithin {
				return fmt.Errorf("no CI run on %s after %s. Check the PR's Checks tab, then land again: pnpm land", head[:12], appearWithin)
			}
		case verdictPending:
			if !announced {
				say("Waiting for %s: https://github.com/%s/actions/runs/%d", ciCheck, repo, run.ID)
				announced = true
			}
		}
		if time.Since(start) > timeout {
			return fmt.Errorf("CI is still running after %s; land again to keep waiting: pnpm land", timeout)
		}
		time.Sleep(pollEvery)
	}
}

// ciFailure names the failed jobs and how to re-run them; flakes are expected, so that's the
// first thing to try.
func ciFailure(r runner, run workflowRun) error {
	var view struct {
		Jobs []struct {
			Name       string `json:"name"`
			Conclusion string `json:"conclusion"`
			URL        string `json:"url"`
		} `json:"jobs"`
	}
	msg := fmt.Sprintf("CI is red: https://github.com/%s/actions/runs/%d\n", repo, run.ID)
	if err := r.ghJSON(&view, "run", "view", fmt.Sprint(run.ID), "-R", repo, "--json", "jobs"); err == nil {
		for _, job := range view.Jobs {
			if job.Conclusion != "success" && job.Conclusion != "skipped" {
				msg += fmt.Sprintf("  %s: %s %s\n", job.Name, job.Conclusion, job.URL)
			}
		}
	}
	msg += fmt.Sprintf("A flake? Re-run the failed jobs, then land again:\n  gh run rerun %d --failed -R %s && pnpm land\nA real failure? Fix it, commit, and land again.", run.ID, repo)
	return errors.New(msg)
}

// landOnMain fast-forwards origin's `main` to the green SHA. `--no-verify`: CI already passed on
// this exact SHA, and a pre-push hook that adds a commit would land something CI never saw.
func landOnMain(r runner, head string) error {
	_, err := r.git("push", "--quiet", "--no-verify", "origin", head+":refs/heads/main")
	if err == nil {
		return nil
	}
	if _, fetchErr := r.git("fetch", "--quiet", "origin", "main"); fetchErr == nil && !r.isAncestor("origin/main", head) {
		return errMainMoved
	}
	return fmt.Errorf("origin refused the land:\n%w", err)
}

// finish syncs local `main` with origin's and deletes the landed remote branch. Both are
// best-effort: the land already happened, so trouble here is a message, not a failure.
func finish(r runner, branch string) error {
	syncMain(r)
	head, err := r.git("rev-parse", "HEAD")
	if err != nil {
		return err
	}
	remote, err := r.remoteSHA(branch)
	if err != nil {
		return err
	}
	switch {
	case remote == "":
	case remote == head:
		if _, err := r.git("push", "--quiet", "--no-verify", "origin", "--delete", branch); err != nil {
			say("Couldn't delete origin/%s: %v", branch, err)
		}
	default:
		say("Left origin/%s alone: it points at %s, which isn't what landed.", branch, remote[:12])
	}
	if teardown := teardownCommand(r.dir); teardown != "" {
		say("Done. This worktree's branch is on main now; remove the worktree when you're finished with it:\n  %s", teardown)
	} else {
		say("Done.")
	}
	return nil
}

// syncMain fast-forwards local `main` to `origin/main`, never checking it out here: wherever it's
// checked out, that worktree moves only when it's clean.
func syncMain(r runner) {
	if _, err := r.git("fetch", "--quiet", "origin", "main"); err != nil {
		say("Couldn't fetch origin/main: %v", err)
		return
	}
	list, err := r.git("worktree", "list", "--porcelain")
	if err != nil {
		say("Couldn't list worktrees: %v", err)
		return
	}
	wt, checkedOut := worktreeOf(parseWorktrees(list), "main")
	if !checkedOut {
		if _, err := r.git("fetch", "--quiet", "origin", "main:main"); err != nil {
			say("Local main didn't fast-forward; see what it has that origin lacks: git log origin/main..main\n%v", err)
			return
		}
		say("Local main is at origin/main.")
		return
	}
	at := runner{dir: wt.path}
	if status, err := at.git("status", "--porcelain"); err != nil || status != "" {
		say("main is checked out at %s with uncommitted changes, so I left it. Fast-forward it when it's clean:\n  git -C %s merge --ff-only origin/main", wt.path, wt.path)
		return
	}
	if _, err := at.git("merge", "--quiet", "--ff-only", "origin/main"); err != nil {
		say("main at %s didn't fast-forward; see what it has that origin lacks: git -C %s log origin/main..main\n%v", wt.path, wt.path, err)
		return
	}
	say("Local main (%s) is at origin/main.", wt.path)
}

func say(format string, args ...any) {
	fmt.Printf(format+"\n", args...)
}

func plural(n int, singular, pluralForm string) string {
	if n == 1 {
		return singular
	}
	return pluralForm
}

func fail(err error) {
	fmt.Fprintln(os.Stderr, err)
	os.Exit(1)
}
