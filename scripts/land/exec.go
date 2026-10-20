package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"os/exec"
	"slices"
	"strings"
)

// runner runs git and gh in one directory, the worktree being landed.
type runner struct {
	dir string
}

func (r runner) cmd(stdin, name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	cmd.Dir = r.dir
	if stdin != "" {
		cmd.Stdin = strings.NewReader(stdin)
	}
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		return stdout.String(), fmt.Errorf("%s %s: %w\n%s", name, strings.Join(args, " "), err, strings.TrimSpace(stderr.String()))
	}
	return stdout.String(), nil
}

// git runs a git command and returns its trimmed stdout.
func (r runner) git(args ...string) (string, error) {
	out, err := r.cmd("", "git", args...)
	return strings.TrimSpace(out), err
}

func (r runner) gitStdin(stdin string, args ...string) error {
	_, err := r.cmd(stdin, "git", args...)
	return err
}

// isAncestor reports whether commit a is reachable from b.
func (r runner) isAncestor(a, b string) bool {
	_, err := r.git("merge-base", "--is-ancestor", a, b)
	return err == nil
}

// remoteSHA is what the branch points at on origin, or "" when origin has no such branch.
func (r runner) remoteSHA(branch string) (string, error) {
	out, err := r.git("ls-remote", "origin", "refs/heads/"+branch)
	if err != nil {
		return "", err
	}
	sha, _, _ := strings.Cut(out, "\t")
	return sha, nil
}

// fetchBranch points `refs/remotes/origin/<branch>` at origin's branch, or deletes it when origin
// has no such branch (deleted after an earlier land), and returns the SHA, or "".
func (r runner) fetchBranch(branch string) (string, error) {
	sha, err := r.remoteSHA(branch)
	if err != nil {
		return "", err
	}
	tracking := "refs/remotes/origin/" + branch
	if sha == "" {
		_, err = r.git("update-ref", "-d", tracking)
	} else {
		_, err = r.git("fetch", "--quiet", "origin", fmt.Sprintf("+refs/heads/%s:%s", branch, tracking))
	}
	return sha, err
}

// inReflog reports whether the local branch ever pointed at sha, as before a rebase or squash.
func (r runner) inReflog(branch, sha string) bool {
	out, err := r.git("log", "--walk-reflogs", "--format=%H", "refs/heads/"+branch)
	if err != nil {
		return false
	}
	return slices.Contains(strings.Split(out, "\n"), sha)
}

// commits lists the commit messages in a range, oldest first.
func (r runner) commits(rangeSpec string) ([]commit, error) {
	out, err := r.git("log", "--reverse", "--format=%s%x00%b%x1e", rangeSpec)
	if err != nil {
		return nil, err
	}
	var commits []commit
	for record := range strings.SplitSeq(out, "\x1e") {
		subject, body, _ := strings.Cut(strings.TrimSpace(record), "\x00")
		if subject != "" {
			commits = append(commits, commit{Subject: subject, Body: strings.TrimSpace(body)})
		}
	}
	return commits, nil
}

// gh runs a gh command and decodes its JSON stdout into out.
func (r runner) ghJSON(out any, args ...string) error {
	stdout, err := r.cmd("", "gh", args...)
	if err != nil {
		return err
	}
	return json.Unmarshal([]byte(stdout), out)
}

func (r runner) gh(args ...string) (string, error) {
	out, err := r.cmd("", "gh", args...)
	return strings.TrimSpace(out), err
}
