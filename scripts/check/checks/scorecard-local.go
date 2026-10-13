package checks

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"slices"
	"sort"
	"strings"
)

const scorecardBaselineRel = "scripts/check/checks/scorecard-local-baseline.json"

// scorecardBaselineFile holds, per file-based OpenSSF Scorecard check, the floor its score must
// hold and the warnings we accept. Its keys are also the list of checks to run.
type scorecardBaselineFile struct {
	Comment string                    `json:"$comment"`
	Checks  map[string]scorecardFloor `json:"checks"`
}

type scorecardFloor struct {
	Floor    int                 `json:"floor"`
	Accepted []scorecardAccepted `json:"accepted,omitempty"`
}

// scorecardAccepted is one `Warn:` finding we live with, written without its line number so an
// edit elsewhere in the file doesn't churn it.
type scorecardAccepted struct {
	Warning string `json:"warning"`
	Reason  string `json:"reason"`
}

type scorecardCheckResult struct {
	Name    string   `json:"name"`
	Score   int      `json:"score"`
	Reason  string   `json:"reason"`
	Details []string `json:"details"`
}

// RunScorecardLocal runs Scorecard's own file-based checks on the tracked tree. It fails on a
// score below its floor and on any `Warn:` finding the baseline doesn't accept, so a new
// unpinned `RUN npm install`, a `FROM` without a digest, or a committed binary fails here before
// the public score moves. Findings, not just scores: Pinned-Dependencies already sits at 9, and
// one more unpinned image still rounds to 9.
//
// Only the checks that read files belong in the baseline: the rest need GitHub's API (branch
// protection, reviews, releases), and Vulnerabilities is `osv-scanner`'s job, with its fix-age
// grace. A local run raises a floor that's beaten and drops an accepted warning that's gone,
// like the allowlists' shrink-wrap; CI fails on either instead, so the file stays honest.
func RunScorecardLocal(ctx *CheckContext) (CheckResult, error) {
	baselinePath := filepath.Join(ctx.RootDir, scorecardBaselineRel)
	raw, err := os.ReadFile(baselinePath)
	if err != nil {
		return CheckResult{}, fmt.Errorf("couldn't read the Scorecard baseline: %w", err)
	}
	var baseline scorecardBaselineFile
	if err := json.Unmarshal(raw, &baseline); err != nil || len(baseline.Checks) == 0 {
		return CheckResult{}, fmt.Errorf("%s has no checks to hold (%v)", scorecardBaselineRel, err)
	}

	scorecard, err := EnsureGoTool(ctx.RootDir, "scorecard", "github.com/ossf/scorecard/v5@v5.5.0")
	if err != nil {
		return CheckResult{}, err
	}
	tree, cleanup, err := exportTrackedTree(ctx.RootDir)
	if err != nil {
		return CheckResult{}, err
	}
	defer cleanup()

	cmd := exec.Command(scorecard, "--local="+tree, "--checks="+strings.Join(sortedKeys(baseline.Checks), ","),
		"--format=json", "--show-details")
	stdout, err := cmd.Output()
	// A check that errors makes scorecard exit non-zero, but the JSON still carries every result
	// (the errored one at -1), and judging that says more than the exit code.
	var report struct {
		Checks []scorecardCheckResult `json:"checks"`
	}
	if jsonErr := json.Unmarshal(stdout, &report); jsonErr != nil {
		return CheckResult{}, fmt.Errorf("scorecard produced no report: %v %v\n%s", err, jsonErr, indentOutput(string(stderrOf(err))))
	}

	verdict := judgeScorecard(report.Checks, baseline.Checks)
	problems := verdict.regressions
	if ctx.CI {
		for _, s := range verdict.stale {
			problems = append(problems, "accepted but no longer reported (run `pnpm check scorecard` to drop it): "+s)
		}
		for name, score := range verdict.raised {
			problems = append(problems, fmt.Sprintf("%s scores %d, above its floor (run `pnpm check scorecard` to raise it)", name, score))
		}
	}
	if len(problems) > 0 {
		sort.Strings(problems)
		problems = append(problems, "",
			"Fix the finding, or, if it's deliberate, accept it with a reason in "+scorecardBaselineRel+" (that loosens an allowlist: ask David).")
		return CheckResult{}, fmt.Errorf("OpenSSF Scorecard findings\n%s", indentOutput(strings.Join(problems, "\n")))
	}

	summary := fmt.Sprintf("%d Scorecard checks at their floor, no new findings", len(baseline.Checks))
	if ctx.CI || (len(verdict.raised) == 0 && len(verdict.stale) == 0) {
		return Success(summary), nil
	}
	changes := tightenScorecardBaseline(baseline.Checks, verdict)
	if err := writeJSONAllowlist(baselinePath, baseline); err != nil {
		return CheckResult{}, err
	}
	reformatWithOxfmt(ctx.RootDir, scorecardBaselineRel)
	return SuccessWithChanges(fmt.Sprintf("%s; tightened the baseline: %s", summary, strings.Join(changes, ", "))), nil
}

type scorecardVerdict struct {
	regressions []string       // drops, errors, missing checks, and new findings
	raised      map[string]int // checks scoring above their floor
	stale       []string       // "Check: warning" pairs accepted but no longer reported
}

// A trailing `:12` (a line number; `:0` is Scorecard's "whole file"), plus the remediation hint
// some findings append after it (`: pin your Docker image by updating ...`).
var scorecardLineSuffixRE = regexp.MustCompile(`:\d+(?:: .*)?$`)

// judgeScorecard compares each result with its floor and its accepted warnings.
func judgeScorecard(results []scorecardCheckResult, baseline map[string]scorecardFloor) scorecardVerdict {
	v := scorecardVerdict{raised: map[string]int{}}
	seen := map[string]bool{}
	for _, r := range results {
		floor, tracked := baseline[r.Name]
		if !tracked {
			continue
		}
		seen[r.Name] = true
		switch {
		case r.Score < 0:
			v.regressions = append(v.regressions, fmt.Sprintf("%s couldn't be scored: %s", r.Name, r.Reason))
			continue
		case r.Score < floor.Floor:
			v.regressions = append(v.regressions, fmt.Sprintf("%s %d < %d: %s", r.Name, r.Score, floor.Floor, r.Reason))
		case r.Score > floor.Floor:
			v.raised[r.Name] = r.Score
		}

		accepted := map[string]bool{}
		for _, a := range floor.Accepted {
			accepted[a.Warning] = false
		}
		for _, d := range r.Details {
			if !strings.HasPrefix(d, "Warn:") {
				continue
			}
			key := scorecardLineSuffixRE.ReplaceAllString(d, "")
			if _, ok := accepted[key]; ok {
				accepted[key] = true
				continue
			}
			v.regressions = append(v.regressions, fmt.Sprintf("%s: new finding: %s", r.Name, d))
		}
		for _, warning := range sortedKeys(accepted) {
			if !accepted[warning] {
				v.stale = append(v.stale, r.Name+": "+warning)
			}
		}
	}
	for _, name := range sortedKeys(baseline) {
		if !seen[name] {
			v.regressions = append(v.regressions, fmt.Sprintf("%s wasn't scored (renamed or dropped by Scorecard?)", name))
		}
	}
	return v
}

// tightenScorecardBaseline raises beaten floors and drops stale acceptances, returning what it
// changed.
func tightenScorecardBaseline(checks map[string]scorecardFloor, v scorecardVerdict) []string {
	var changes []string
	for name, score := range v.raised {
		floor := checks[name]
		floor.Floor = score
		checks[name] = floor
		changes = append(changes, fmt.Sprintf("raised %s to %d", name, score))
	}
	for _, s := range v.stale {
		name, warning, _ := strings.Cut(s, ": ")
		floor := checks[name]
		floor.Accepted = slices.DeleteFunc(floor.Accepted, func(a scorecardAccepted) bool { return a.Warning == warning })
		checks[name] = floor
		changes = append(changes, "dropped "+s)
	}
	sort.Strings(changes)
	return changes
}
