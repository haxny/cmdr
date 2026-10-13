package checks

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
)

// exportTrackedTree copies the files git would commit (tracked, plus untracked ones not
// ignored) into a fresh temp dir, and returns it with its cleanup. The Scorecard lanes scan this
// copy, not the worktree, for two reasons:
//   - It's what Scorecard sees: a clone has no `target/`, `node_modules/`, or sockets. Pointed at
//     the worktree, `scorecard --local` walks all of those (68 s) and dies on `.codegraph`'s socket.
//   - osv-scanner's walk skips paths the enclosing repo ignores, and every worktree lives under
//     the main clone's ignored `.claude/worktrees/`, so in a worktree it found nothing at all
//     (verified with osv-scanner v2.6.0, 2026-10-13).
//
// The temp dir sits outside any repo for that second reason. Copying ~8,000 files (~93 MB)
// takes about a second.
func exportTrackedTree(rootDir string) (string, func(), error) {
	cmd := exec.Command("git", "ls-files", "-z", "--cached", "--others", "--exclude-standard")
	cmd.Dir = rootDir
	out, err := cmd.Output()
	if err != nil {
		return "", nil, fmt.Errorf("listing tracked files: %w", err)
	}

	dest, err := os.MkdirTemp("", "cmdr-tracked-tree-")
	if err != nil {
		return "", nil, err
	}
	cleanup := func() { _ = os.RemoveAll(dest) }

	for _, rel := range bytes.Split(bytes.TrimRight(out, "\x00"), []byte{0}) {
		if len(rel) == 0 {
			continue
		}
		if err := copyTrackedEntry(filepath.Join(rootDir, string(rel)), filepath.Join(dest, string(rel))); err != nil {
			cleanup()
			return "", nil, err
		}
	}
	return dest, cleanup, nil
}

// copyTrackedEntry copies one file or symlink. A path git lists but the disk lacks (deleted,
// not yet staged) is skipped, as is a submodule's directory.
func copyTrackedEntry(src, dst string) error {
	info, err := os.Lstat(src)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(dst), 0o755); err != nil {
		return err
	}
	switch {
	case info.Mode()&os.ModeSymlink != 0:
		target, err := os.Readlink(src)
		if err != nil {
			return err
		}
		return os.Symlink(target, dst)
	case info.Mode().IsRegular():
		return copyRegularFile(src, dst, info.Mode().Perm())
	default:
		return nil
	}
}

func copyRegularFile(src, dst string, perm os.FileMode) error {
	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer in.Close()
	out, err := os.OpenFile(dst, os.O_CREATE|os.O_WRONLY|os.O_TRUNC, perm)
	if err != nil {
		return err
	}
	if _, err := io.Copy(out, in); err != nil {
		out.Close()
		return err
	}
	return out.Close()
}
