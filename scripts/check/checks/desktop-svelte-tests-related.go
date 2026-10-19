package checks

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
)

// relatedSourceDirs are the trees, relative to the repo root, that hold the
// desktop vitest suite's code: the same set `vitest.config.ts` includes tests
// from. A change anywhere else can't change what those tests import.
var relatedSourceDirs = []string{
	"apps/desktop/src/",
	"apps/desktop/scripts/",
	"apps/desktop/eslint-plugins/",
	"apps/desktop/test/",
	"eslint-plugins/",
}

var relatedSourceExts = map[string]bool{".ts": true, ".js": true, ".mjs": true, ".svelte": true}

// relatedTestTargets turns the branch's changed repo paths into the module paths
// `vitest related` takes, relative to `apps/desktop`, keeping only code the
// desktop suite can import. Order follows the input; duplicates drop.
func relatedTestTargets(changed []string) []string {
	var targets []string
	seen := map[string]bool{}
	for _, path := range changed {
		if path == "" || seen[path] || !relatedSourceExts[filepath.Ext(path)] {
			continue
		}
		for _, dir := range relatedSourceDirs {
			if !strings.HasPrefix(path, dir) {
				continue
			}
			seen[path] = true
			if rel, ok := strings.CutPrefix(path, "apps/desktop/"); ok {
				targets = append(targets, rel)
			} else {
				targets = append(targets, "../../"+path)
			}
			break
		}
	}
	return targets
}

// branchChangedFiles lists what this branch changed against `origin/main`:
// committed, staged, unstaged, and untracked alike, so the lane works before a
// commit and after one. Without an `origin/main` (a fresh clone with no fetch),
// it falls back to the uncommitted changes alone.
func branchChangedFiles(rootDir string) ([]string, error) {
	base := "HEAD"
	if out, err := runGit(rootDir, "merge-base", "HEAD", "origin/main"); err == nil {
		base = strings.TrimSpace(out)
	}
	diff, err := runGit(rootDir, "diff", "--name-only", "--diff-filter=d", base)
	if err != nil {
		return nil, err
	}
	untracked, err := runGit(rootDir, "ls-files", "--others", "--exclude-standard")
	if err != nil {
		return nil, err
	}
	return strings.Split(diff+"\n"+untracked, "\n"), nil
}

var vitestPassedRe = regexp.MustCompile(`Tests\s+(\d+) passed`)

// RunSvelteTestsRelated runs only the desktop vitest files whose imports reach
// what this branch changed, a seconds-to-a-minute subset of `svelte-tests` for
// an agent to run beside `--fast`. Named-only: the full suite gates in CI.
func RunSvelteTestsRelated(ctx *CheckContext) (CheckResult, error) {
	changed, err := branchChangedFiles(ctx.RootDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read what this branch changed: %w", err)
	}
	targets := relatedTestTargets(changed)
	if len(targets) == 0 {
		return Skipped("no desktop frontend code changed"), nil
	}

	desktopDir := filepath.Join(ctx.RootDir, "apps", "desktop")
	reportDir, err := os.MkdirTemp("", "cmdr-svelte-related-*")
	if err != nil {
		return CheckResult{}, fmt.Errorf("failed to create the report dir: %w", err)
	}
	defer os.RemoveAll(reportDir)
	jsonReport := filepath.Join(reportDir, "test-results.json")

	args := append([]string{"exec", "vitest", "related", "--run", "--passWithNoTests"}, targets...)
	cmd := exec.Command("pnpm", args...)
	cmd.Dir = desktopDir
	cmd.Env = append(os.Environ(), "VITEST_JSON_REPORT="+jsonReport)
	output, err := RunCommand(cmd, true)
	recordVitestTests(ctx, jsonReport, desktopDir)
	if err != nil {
		return CheckResult{}, fmt.Errorf("related svelte tests failed: %s", diagnoseVitestFailure(jsonReport, desktopDir, output))
	}

	files := fmt.Sprintf("%d changed %s", len(targets), Pluralize(len(targets), "file", "files"))
	match := vitestPassedRe.FindStringSubmatch(StripANSI(output))
	if match == nil {
		return Success("no tests reach the " + files), nil
	}
	count, _ := strconv.Atoi(match[1])
	result := Success(fmt.Sprintf("%d %s reaching the %s passed", count, Pluralize(count, "test", "tests"), files))
	result.Total = count
	return result, nil
}
