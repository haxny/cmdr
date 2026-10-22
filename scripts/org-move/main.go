// org-move rewrites the repo's identity from `vdavid/cmdr` to `getcmdr/cmdr` across the tracked
// files, for the move to the `getcmdr` GitHub org (#476). Run it on transfer day, on a worktree
// branched from fresh `main`, then let `pnpm check` rewrap what grew past the line width.
//
// It's idempotent: a second run changes nothing. It prints every file it changed, every
// hand-written edit, and every mention it left on the old name with the reason. A mention no rule
// explains, or an edit whose anchor is gone, fails the run so a person decides. The policy lives
// in `rules.go`; the runbook is `docs/notes/github-org-move.md`.
//
// Usage: ./scripts/org-move.sh -cutoff vX.Y.Z [-dry-run] [-v]
//
// `-cutoff` is the last release built under `vdavid/cmdr`, named on `/trust` as the last one whose
// attestations name the old repo.
package main

import (
	"flag"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"sort"
	"strings"
)

var cutoffRE = regexp.MustCompile(`^v\d+\.\d+\.\d+$`)

type editResult struct {
	path   string
	status editStatus
	why    string
}

type report struct {
	changed      map[string]int            // path -> mentions swapped
	edits        []editResult              // every edit, per file
	kept         map[string]map[string]int // reason -> path -> mentions
	unclassified map[string]int            // path -> mentions no rule covers
}

func main() {
	root := flag.String("root", ".", "repo root")
	cutoff := flag.String("cutoff", "", "the last release built under vdavid/cmdr, like v0.55.0")
	dryRun := flag.Bool("dry-run", false, "print what would change, write nothing")
	verbose := flag.Bool("v", false, "list the files behind each keep reason")
	flag.Parse()

	if !cutoffRE.MatchString(*cutoff) {
		fail("-cutoff must be a release tag like v0.55.0, got %q", *cutoff)
	}
	files, err := candidateFiles(*root)
	if err != nil {
		fail("%v", err)
	}
	rep, err := run(*root, files, *cutoff, *dryRun)
	if err != nil {
		fail("%v", err)
	}
	if !rep.print(*cutoff, *dryRun, *verbose) {
		os.Exit(1)
	}
}

// candidateFiles lists the tracked text files that mention `vdavid` or carry an edit.
func candidateFiles(root string) ([]string, error) {
	cmd := exec.Command("git", "grep", "-l", "-z", "-I", "vdavid")
	cmd.Dir = root
	out, err := cmd.Output()
	if err != nil {
		if ee, ok := err.(*exec.ExitError); !ok || ee.ExitCode() != 1 {
			return nil, fmt.Errorf("git grep: %w", err)
		}
	}
	files := strings.Split(strings.TrimRight(string(out), "\x00"), "\x00")
	for _, e := range edits {
		files = append(files, e.files...)
	}
	slices.Sort(files)
	return slices.Compact(slices.DeleteFunc(files, func(f string) bool { return f == "" })), nil
}

func run(root string, files []string, cutoff string, dryRun bool) (report, error) {
	rep := report{changed: map[string]int{}, kept: map[string]map[string]int{}, unclassified: map[string]int{}}
	for _, path := range files {
		full := filepath.Join(root, path)
		raw, err := os.ReadFile(full)
		if os.IsNotExist(err) {
			for _, e := range editsFor(path) {
				rep.edits = append(rep.edits, editResult{path, editAnchorMissing, e.why + " (file is gone)"})
			}
			continue
		}
		if err != nil {
			return rep, err
		}
		orig := string(raw)
		content := rep.processFile(path, orig, cutoff)
		if content != orig && !dryRun {
			info, err := os.Stat(full)
			if err != nil {
				return rep, err
			}
			if err := os.WriteFile(full, []byte(content), info.Mode().Perm()); err != nil {
				return rep, err
			}
		}
	}
	return rep, nil
}

// processFile applies the file's edits, then the generic swap if the rules allow it, records
// what happened, and returns the new text.
func (r *report) processFile(path, content, cutoff string) string {
	for _, e := range editsFor(path) {
		var status editStatus
		content, status = applyEdit(content, e, cutoff)
		r.edits = append(r.edits, editResult{path, status, e.why})
	}
	var protected []span
	for _, e := range editsFor(path) {
		protected = append(protected, editSpans(content, e, cutoff)...)
	}

	swapped, res := swapContent(content, protected)
	if why, excluded := exclusionFor(path); excluded {
		// Everything in an excluded file stays for the file's reason, whatever else applies.
		n := res.swapped
		for _, k := range res.kept {
			n += k
		}
		r.keep(why, path, n)
		return content
	}
	for why, n := range res.kept {
		r.keep(why, path, n)
	}
	if !included(path) {
		if res.swapped > 0 {
			r.unclassified[path] = res.swapped
		}
		return content
	}
	if res.swapped > 0 {
		r.changed[path] = res.swapped
	}
	return swapped
}

func editsFor(path string) []edit {
	var out []edit
	for _, e := range edits {
		if slices.Contains(e.files, path) {
			out = append(out, e)
		}
	}
	return out
}

func exclusionFor(path string) (string, bool) {
	for _, x := range excludedPaths {
		if globMatch(x.glob, path) {
			return x.why, true
		}
	}
	return "", false
}

func included(path string) bool {
	return slices.ContainsFunc(includeGlobs, func(g string) bool { return globMatch(g, path) })
}

func (r *report) keep(why, path string, n int) {
	if n == 0 {
		return
	}
	if r.kept[why] == nil {
		r.kept[why] = map[string]int{}
	}
	r.kept[why][path] += n
}

// print writes the report and says whether the run is clean.
func (r report) print(cutoff string, dryRun, verbose bool) bool {
	verb := "Swapped"
	if dryRun {
		verb = "Would swap"
	}
	fmt.Printf("org-move: %s -> %s (last release under the old name: %s)\n\n", oldRepo, newRepo, cutoff)

	total := 0
	for _, n := range r.changed {
		total += n
	}
	fmt.Printf("%s %d mentions in %d files:\n", verb, total, len(r.changed))
	for _, p := range sortedKeys(r.changed) {
		fmt.Printf("  %s (%d)\n", p, r.changed[p])
	}

	ok := true
	fmt.Printf("\nEdits:\n")
	for _, e := range r.edits {
		fmt.Printf("  %-15s %s: %s\n", e.status, e.path, e.why)
		if e.status == editAnchorMissing {
			ok = false
		}
	}

	fmt.Printf("\nKept on the old name:\n")
	reasons := make([]string, 0, len(r.kept))
	for why := range r.kept {
		reasons = append(reasons, why)
	}
	sort.Strings(reasons)
	for _, why := range reasons {
		n := 0
		for _, c := range r.kept[why] {
			n += c
		}
		fmt.Printf("  %d in %d files: %s\n", n, len(r.kept[why]), why)
		if verbose {
			for _, p := range sortedKeys(r.kept[why]) {
				fmt.Printf("      %s (%d)\n", p, r.kept[why][p])
			}
		}
	}

	if len(r.unclassified) > 0 {
		ok = false
		fmt.Printf("\nUnclassified (in neither includeGlobs nor excludedPaths in rules.go, so untouched):\n")
		for _, p := range sortedKeys(r.unclassified) {
			fmt.Printf("  %s (%d)\n", p, r.unclassified[p])
		}
	}
	if !ok {
		fmt.Printf("\nNeeds a person: fix the rules in scripts/org-move/rules.go (or the files by hand) and re-run.\n")
	}
	return ok
}

func sortedKeys(m map[string]int) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

func fail(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "org-move: "+format+"\n", args...)
	os.Exit(2)
}
