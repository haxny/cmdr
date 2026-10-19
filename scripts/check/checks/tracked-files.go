package checks

import (
	"fmt"
	"os"
	"path/filepath"
	"slices"
)

// The shared vocabulary for "what files does git know about, and how does a
// scanner read one". Every whole-tree or prefix-scoped scanner enumerates
// through here (or through `listTrackedFiles` for a prefix) so they all agree on
// what "in the repo" means, and reads through `readTrackedFile` so none of them
// turns red over a deletion that isn't staged yet. Both go through `corpus`, so
// the listing and each file's bytes are fetched once per run.

// repoFiles enumerates every first-party file as a repo-relative, forward-slashed
// path: the git-tracked set when rootDir is a work tree (so gitignored and
// untracked generated output is excluded for free), else a filesystem walk. Both
// whole-tree scanners (`file-length`, `invariant-density`) take their file list
// from here so they agree on what "in the repo" means.
func repoFiles(rootDir string) ([]string, error) {
	if relPaths, ok := gitTrackedFiles(rootDir); ok {
		return relPaths, nil
	}
	return walkSourceFiles(rootDir)
}

// gitTrackedFiles returns every tracked file as a repo-relative, forward-slashed
// path, as the caller's own copy. Returns (nil, false) when rootDir isn't a git
// work tree, so the caller can fall back to a filesystem walk. Tracked-only (no
// `--others`) is the whole point: gitignored and untracked generated output
// never reaches the scanner.
func gitTrackedFiles(rootDir string) ([]string, bool) {
	files, err := corpus.GitLsFiles(rootDir)
	if err != nil {
		return nil, false
	}
	return slices.Clone(files), true
}

// readTrackedFile reads one file from a git listing, reporting ok=false when
// it's no longer on disk.
//
// Gotcha: git keeps listing a tracked file until its deletion is STAGED, so
// between `rm` and `git add` every scanner gets a path with nothing behind it.
// A scanner that let that read error escape turned `pnpm check --fast` red with
// `open <path>: no such file or directory` on work that had nothing to do with
// it (hit for real deleting a locale JSON). The skip is silent on purpose: a
// file that isn't there has nothing to report, and the deletion is the user's
// business, not the lane's. Any OTHER read error is real and comes back wrapped.
func readTrackedFile(rootDir, rel string) (string, bool, error) {
	text, err := corpus.ReadFile(filepath.Join(rootDir, filepath.FromSlash(rel)))
	switch {
	case os.IsNotExist(err):
		return "", false, nil
	case err != nil:
		return "", false, fmt.Errorf("read %s: %w", rel, err)
	}
	return text, true, nil
}
